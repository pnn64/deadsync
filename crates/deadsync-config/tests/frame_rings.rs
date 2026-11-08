use deadsync_config::frame_pacing::{FixedFrameStatsRing, StutterSampleRing};
use std::cell::RefCell;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "support/frame_rings_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn stutter_ring_preserves_wrapping_filtering_float_bits_and_clear() {
    let values = [
        f32::NEG_INFINITY,
        -4.0,
        -0.0,
        0.0,
        0.001,
        1.0,
        3.4,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
    ];
    let mut current = StutterSampleRing::new();
    let mut old = original::StutterSampleRing::new();
    assert!(current.visible(0.0).is_empty());
    for i in 0..4096 {
        if i % 31 == 0 {
            current.clear();
            old.clear();
        }
        let at = values[i % values.len()];
        let frame = values[(i / 3) % values.len()];
        let expected = values[(i / 7) % values.len()];
        current.push(at, frame, expected, i as u8);
        old.push(at, frame, expected, i as u8);
        for now in values.into_iter().chain([
            at + 3.4,
            f32::from_bits((at + 3.4).to_bits().wrapping_add(1)),
        ]) {
            let a = current.visible(now);
            let b = old.visible(now);
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b.iter()) {
                assert_eq!(a.timestamp_seconds.to_bits(), b.timestamp_seconds.to_bits());
                assert_eq!(a.frame_ms.to_bits(), b.frame_ms.to_bits());
                assert_eq!(a.frame_multiple.to_bits(), b.frame_multiple.to_bits());
                assert_eq!(a.age_seconds.to_bits(), b.age_seconds.to_bits());
                assert_eq!(a.severity, b.severity);
            }
        }
    }
}

fn check_history<const N: usize>() {
    let mut current = FixedFrameStatsRing::<[u64; 2], N>::new([0; 2]);
    let mut old = original::FixedFrameStatsRing::<[u64; 2], N>::new([0; 2]);
    for phase in 0..2 {
        current.clear();
        old.clear();
        for i in 0..N * 2 + 7 {
            let sample = [
                if i % 7 == 0 { 0 } else { i as u64 * 11 },
                phase * 1000 + i as u64,
            ];
            current.push(sample);
            old.push(sample);
            let mut a = Vec::new();
            let mut b = Vec::new();
            current.snapshot(&mut a);
            old.snapshot(&mut b);
            assert_eq!(a, b);
            for (now, window) in [(0, 0), (30, 15), (u64::MAX, u64::MAX), (i as u64 * 11, 70)] {
                for panic_at in [usize::MAX, 1, N / 2 + 1] {
                    let mut results = Vec::new();
                    for reference in [true, false] {
                        let calls = RefCell::new(Vec::new());
                        let mut out = vec![[999; 2]; 3];
                        let timestamp = |sample: [u64; 2]| {
                            let mut calls = calls.borrow_mut();
                            calls.push(sample);
                            assert_ne!(calls.len(), panic_at, "timestamp panic");
                            sample[0]
                        };
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            if reference {
                                old.collect_recent_by(now, window, &mut out, timestamp);
                            } else {
                                current.collect_recent_by(now, window, &mut out, timestamp);
                            }
                        }));
                        results.push((result.is_err(), out.capacity(), out, calls.into_inner()));
                    }
                    assert_eq!(
                        results[0], results[1],
                        "N={N} phase={phase} i={i} now={now} window={window}"
                    );
                }
            }
        }
    }
}

#[test]
fn recent_history_preserves_order_callback_trace_capacity_and_panic_prefix() {
    check_history::<0>();
    check_history::<1>();
    check_history::<2>();
    check_history::<3>();
    check_history::<5>();
    check_history::<7>();
    check_history::<8>();
    check_history::<127>();
    check_history::<128>();
    check_history::<129>();
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

fn history_benchmark<T: Copy, const N: usize>(
    label: &str,
    empty: T,
    sample: impl Fn(usize) -> T,
    timestamp: impl Fn(T) -> u64,
) {
    let mut ring = FixedFrameStatsRing::<T, N>::new(empty);
    for i in 0..N + 37 {
        ring.push(sample(i));
    }
    let mut out = Vec::with_capacity(N);
    measure(&format!("recent/{label}"), 256, || {
        ring.collect_recent_by(
            black_box(2_000_000_000),
            black_box(500_000_000),
            black_box(&mut out),
            |value| timestamp(black_box(value)),
        );
        black_box(&out);
    });
    if N == 128 {
        measure(&format!("snapshot/{label}"), 256, || {
            black_box(&ring).snapshot(black_box(&mut out));
            black_box(&out);
        });
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_frame_rings() {
    let entries: Vec<_> = (0..8192)
        .map(|i| (i as f32 / 100.0, 0.04, 1.0 / 60.0, (i % 4) as u8))
        .collect();
    let mut ring = StutterSampleRing::new();
    measure("push/stutter", 32, || {
        for &(at, frame, expected, severity) in black_box(&entries) {
            black_box(&mut ring).push(
                black_box(at),
                black_box(frame),
                black_box(expected),
                black_box(severity),
            );
        }
        black_box(&ring);
    });
    for (label, kind) in [("empty", 0), ("visible", 1), ("filtered", 2), ("mixed", 3)] {
        let rings: Vec<_> = (0..512)
            .map(|i| {
                let mut ring = StutterSampleRing::new();
                if kind != 0 {
                    for j in 0..5 + i % 5 {
                        let at = if kind == 2 || (kind == 3 && j % 2 == 0) {
                            1.0
                        } else {
                            9.0 + j as f32 / 20.0
                        };
                        ring.push(
                            at,
                            0.04,
                            1.0 / 60.0,
                            if kind == 3 && j % 3 == 0 { 0 } else { 2 },
                        );
                    }
                }
                ring
            })
            .collect();
        measure(&format!("visible/{label}"), 32, || {
            for &ring in black_box(&rings) {
                black_box(black_box(ring).visible(black_box(10.0)));
            }
        });
    }
    let timestamp = |i: usize| {
        if i % 11 == 0 {
            0
        } else {
            1_000_000_000 + i as u64 * 5_000_000
        }
    };
    history_benchmark::<u64, 128>("u64_128", 0, timestamp, |value| value);
    history_benchmark::<u64, 129>("u64_129", 0, timestamp, |value| value);
    history_benchmark::<[u64; 24], 128>(
        "wide_128",
        [0; 24],
        |i| {
            let mut value = [i as u64; 24];
            value[0] = timestamp(i);
            value
        },
        |value| value[0],
    );
}

fn history_push_benchmark<T: Copy, const N: usize>(label: &str, empty: T, data: &[T]) {
    let mut ring = FixedFrameStatsRing::<T, N>::new(empty);
    for &value in data.iter().take(N) {
        ring.push(value);
    }
    measure(label, 32, || {
        for &value in black_box(data) {
            black_box(&mut ring).push(black_box(value));
        }
        black_box(&ring);
    });
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_history_push() {
    let scalar: Vec<_> = (0..8192_u64).collect();
    let wide: Vec<_> = (0..8192_u64).map(|i| [i; 24]).collect();
    history_push_benchmark::<u64, 128>("history_push/full_scalar", 0, &scalar);
    history_push_benchmark::<[u64; 24], 128>("history_push/full_wide", [0; 24], &wide);
    history_push_benchmark::<u64, 129>("history_push/full_scalar_129", 0, &scalar);
    history_push_benchmark::<[u64; 24], 129>("history_push/full_wide_129", [0; 24], &wide);
    measure("history_push/growing_scalar", 512, || {
        let mut ring = FixedFrameStatsRing::<u64, 128>::new(0);
        for &value in black_box(&scalar[..64]) {
            black_box(&mut ring).push(black_box(value));
        }
        black_box(&ring);
    });
}
