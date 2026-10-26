//! Run through scripts/benchmark_alsa.py on Linux; never requires audio hardware.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: all operations preserve System's allocation/layout contracts unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: forwarded with the caller's valid layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: forwarded with the caller's valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded with the caller's live allocation and layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        // SAFETY: forwarded with the caller's valid allocation and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn clock_mapping_matches_wide_integer_reference() {
    let values = [
        0,
        1,
        999_999,
        1_000_000,
        5_000_000,
        50_000_000,
        u64::MAX / 2,
        u64::MAX - 1,
        u64::MAX,
    ];
    for host in values {
        for clock in values {
            for target in values {
                check_clock_mapping(host, clock, target);
            }
        }
    }
    let mut state = 0x8c1f_74ea_623b_d905_u64;
    for _ in 0..100_000 {
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            state
        };
        check_clock_mapping(next(), next(), next());
    }
}

fn check_clock_mapping(host: u64, clock: u64, target: u64) {
    let expected = (i128::from(host) + i128::from(target) - i128::from(clock))
        .clamp(0, i128::from(u64::MAX)) as u64;
    assert_eq!(
        host_nanos_from_clock(
            target,
            ClockSample {
                host_nanos: host,
                clock_nanos: clock
            }
        ),
        expected
    );
}

#[test]
fn frame_conversion_matches_integer_division() {
    for rate in [
        0,
        1,
        2,
        3,
        5,
        7,
        8_000,
        44_100,
        48_000,
        96_000,
        192_000,
        384_000,
        u32::MAX,
    ] {
        for frames in [0, 1, 2, 127, 240, 441, 480, u32::MAX - 1, u32::MAX] {
            check_frame_conversion(rate, frames);
        }
    }
    for rate in 0..=384_000 {
        for frames in [0, 1, 480, u32::MAX] {
            check_frame_conversion(rate, frames);
        }
    }
    let mut state = 0xa53b_1cd9_u32;
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state
    };
    for _ in 0..100_000 {
        check_frame_conversion(next(), next());
    }
}

fn check_frame_conversion(rate: u32, frames: u32) {
    let expected = if rate == 0 {
        0
    } else {
        u64::from(frames) * 1_000_000_000 / u64::from(rate)
    };
    let (reciprocal, shift) = sample_rate_scale(rate);
    assert_eq!(
        frames_to_nanos(rate, frames, reciprocal, shift),
        expected,
        "rate={rate} frames={frames}"
    );
}

#[test]
fn pcm_selection_preserves_text_and_errors() {
    for suffix in ["", "0", "CARD=2,DEV=1", "音楽", "x\0y"] {
        for (prefix, shared, exclusive) in
            [("hw:", "plughw:", "hw:"), ("plughw:", "plughw:", "hw:")]
        {
            let input = format!("{prefix}{suffix}");
            assert_eq!(
                selected_pcm_id(Some(input.clone()), "device", AlsaAccessMode::Shared),
                Ok(format!("{shared}{suffix}"))
            );
            assert_eq!(
                selected_pcm_id(Some(input), "device", AlsaAccessMode::Exclusive),
                Ok(format!("{exclusive}{suffix}"))
            );
        }
    }
    for alias in ["", "default", "sysdefault", "null", "plug:dmix", "音楽"] {
        assert_eq!(
            selected_pcm_id(Some(alias.to_owned()), "device", AlsaAccessMode::Shared),
            Ok(alias.to_owned())
        );
    }
    assert_eq!(
        selected_pcm_id(None, "device", AlsaAccessMode::Shared),
        Ok("default".into())
    );
    let alias = "deadsync_missing_pcm_benchmark_fixture";
    let expected = format!(
        "ALSA exclusive output for 'device' requires a direct hw/plughw device or a resolvable hardware-backed alias, got '{alias}'"
    );
    assert_eq!(
        selected_pcm_id(Some(alias.into()), "device", AlsaAccessMode::Exclusive),
        Err(expected)
    );
}

#[test]
#[ignore = "requires the deterministic ALSA enumeration fixture"]
fn fixture_enumeration_preserves_devices() {
    let layout = std::env::var("ALSA_FIXTURE_LAYOUT").unwrap_or_default();
    let devices = enumerate_output_devices();
    if layout == "empty" {
        assert!(devices.is_empty());
        return;
    }
    assert_eq!(devices.len(), 8);
    for (index, device) in devices.iter().enumerate() {
        assert_eq!(
            device.pcm_id,
            format!("hw:CARD={},DEV={}", index / 4, index % 4)
        );
        let name = match layout.as_str() {
            "bad_name" => format!("Fixture card 音楽, Device {}", index % 4),
            "bad_card_info" | "bad_card_name" => "Fallback card 音楽, Fixture playback".to_owned(),
            "missing_card_name" => format!("Card {}, Fixture playback", index / 4),
            "long_names" => format!("{}, {}", "音楽".repeat(32), "音楽".repeat(16)),
            "empty_names" => ", ".to_owned(),
            _ => "Fixture card 音楽, Fixture playback".to_owned(),
        };
        assert_eq!(device.name, name);
        if layout == "probe_error" {
            assert!(device.sample_rates_hz.is_empty());
        } else {
            assert_eq!(
                device.sample_rates_hz,
                [
                    11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
                    192_000, 384_000
                ]
            );
        }
        assert_eq!(device.default_rate_hz, 48_000);
        assert_eq!(device.channels, 2);
        assert_eq!(
            device.is_default,
            index == if layout == "unmatched_default" { 0 } else { 5 }
        );
    }
}

#[test]
#[ignore = "requires the deterministic ALSA timing fixture"]
fn fixture_timing_preserves_delay_and_fallback() {
    let pcm = open_pcm("null").unwrap();
    let actual = configure_pcm(&pcm, 48_000, 2, AlsaAccessMode::Shared, 240, Some(480)).unwrap();
    let expected_clock = if std::env::var_os("ALSA_FIXTURE_MONOTONIC").is_some() {
        OutputTelemetryClock::Monotonic
    } else {
        OutputTelemetryClock::MonotonicRaw
    };
    assert_eq!(actual.host_clock.telemetry_clock(), expected_clock);
    let delay: i64 = std::env::var("ALSA_FIXTURE_DELAY")
        .unwrap()
        .parse()
        .unwrap();
    let frames = delay.max(0) as u32;
    let mut health = AlsaClockHealth::new();
    let timing = playback_status_timing(
        &pcm,
        48_000,
        actual.sample_rate_reciprocal,
        actual.sample_rate_shift,
        actual.host_clock,
        &mut health,
    );
    assert_eq!(timing.delay_frames, frames);
    assert_eq!(
        timing.estimated_output_delay_ns,
        u64::from(frames) * 1_000_000_000 / 48_000
    );
    if std::env::var_os("ALSA_FIXTURE_STATUS_ERROR").is_some() {
        assert_eq!(timing.timing_quality, OutputTimingQuality::Fallback);
        assert_eq!(
            health.degraded_samples_remaining,
            ALSA_DEGRADED_HOLD_SAMPLES
        );
        assert_eq!(health.last_status_host_nanos, 0);
        assert_eq!(health.last_playback_host_nanos, 0);
    } else {
        assert_eq!(timing.timing_quality, OutputTimingQuality::Trusted);
        assert_eq!(
            timing.playback_host_nanos,
            health
                .last_status_host_nanos
                .saturating_add(timing.estimated_output_delay_ns)
        );
    }
}

#[test]
#[ignore = "requires the deterministic ALSA timing fixture"]
fn fixture_clock_health_extremes_preserve_quality() {
    let pcm = open_pcm("null").unwrap();
    let actual = configure_pcm(&pcm, 48_000, 2, AlsaAccessMode::Shared, 240, Some(480)).unwrap();
    for last_status in [0, 1, u64::MAX] {
        for last_playback in [0, 1, u64::MAX] {
            let mut health = AlsaClockHealth {
                last_status_host_nanos: last_status,
                last_playback_host_nanos: last_playback,
                degraded_samples_remaining: 0,
            };
            let timing = playback_status_timing(
                &pcm,
                48_000,
                actual.sample_rate_reciprocal,
                actual.sample_rate_shift,
                actual.host_clock,
                &mut health,
            );
            if last_status == u64::MAX || last_playback == u64::MAX {
                assert_eq!(timing.timing_quality, OutputTimingQuality::Fallback);
                assert_eq!(health.last_status_host_nanos, last_status);
                assert_eq!(health.last_playback_host_nanos, last_playback);
                assert_eq!(
                    health.degraded_samples_remaining,
                    ALSA_DEGRADED_HOLD_SAMPLES
                );
            } else {
                assert_eq!(timing.timing_quality, OutputTimingQuality::Trusted);
                assert_eq!(health.last_playback_host_nanos, timing.playback_host_nanos);
            }
        }
    }
}

#[test]
#[ignore = "requires the deterministic ALSA rate fixture"]
fn fixture_rate_matrix_preserves_capabilities() {
    let priority = [
        48_000, 44_100, 96_000, 88_200, 192_000, 176_400, 32_000, 22_050, 16_000, 11_025, 384_000,
    ];
    for mask in 0..2048_u16 {
        let pcm = open_pcm("null").unwrap();
        let hw = HwParams::any(&pcm).unwrap();
        let (rates, default) = sample_rate_capabilities(&hw).unwrap();
        let mut expected = vec![8_000, 500_000];
        for (index, rate) in priority.into_iter().enumerate() {
            if mask & (1 << index) != 0 {
                expected.push(rate);
            }
        }
        let expected_default = expected.get(2).copied().unwrap_or(8_000);
        expected.sort_unstable();
        assert_eq!(default, expected_default, "mask={mask}");
        assert_eq!(rates, expected, "mask={mask}");
    }
}

#[test]
#[ignore = "requires the deterministic ALSA rate fixture"]
fn fixture_rate_edges_preserve_capabilities() {
    let edge = std::env::var("ALSA_FIXTURE_RATE_EDGE").unwrap();
    let mask: u16 = std::env::var("ALSA_FIXTURE_RATE_MASK")
        .unwrap()
        .parse()
        .unwrap();
    let expected = match (edge.as_str(), mask) {
        ("min_error" | "reject_edges" | "min_max_reversed" | "duplicate_bounds", 0) => None,
        ("min_error", 1) => Some((vec![48_000, 500_000], 48_000)),
        ("max_error", 0) => Some((vec![8_000], 8_000)),
        ("max_error", 1) => Some((vec![8_000, 48_000], 48_000)),
        ("reject_min", 0) => Some((vec![8_000, 500_000], 500_000)),
        ("reject_min" | "reject_edges", 1) => Some((vec![8_000, 48_000, 500_000], 48_000)),
        ("min_zero", 0) => Some((vec![0, 500_000], 500_000)),
        ("min_zero", 1) => Some((vec![0, 48_000, 500_000], 48_000)),
        ("min_max_reversed", 1) => Some((vec![48_000, 500_000, 600_000], 48_000)),
        ("duplicate_bounds", 1) => Some((vec![48_000], 48_000)),
        _ => panic!("unknown rate fixture {edge}/{mask}"),
    };
    let pcm = open_pcm("null").unwrap();
    let hw = HwParams::any(&pcm).unwrap();
    assert_eq!(sample_rate_capabilities(&hw), expected);
}

#[test]
#[ignore = "performance workload; use scripts/benchmark_alsa.py"]
fn benchmark_alsa() {
    let workload = std::env::var("ALSA_PERF_WORKLOAD").unwrap();
    let iterations: usize = std::env::var("ALSA_PERF_ITERATIONS")
        .unwrap()
        .parse()
        .unwrap();
    match workload.as_str() {
        "shared_alias" | "shared_hw" | "exclusive_hw" | "exclusive_plughw" => {
            let (id, mode) = match workload.as_str() {
                "shared_alias" => ("default", AlsaAccessMode::Shared),
                "shared_hw" => ("hw:CARD=2,DEV=1", AlsaAccessMode::Shared),
                "exclusive_hw" => ("hw:CARD=2,DEV=1", AlsaAccessMode::Exclusive),
                _ => ("plughw:CARD=2,DEV=1", AlsaAccessMode::Exclusive),
            };
            run_work(iterations, || {
                black_box(selected_pcm_id(
                    Some(black_box(id).to_owned()),
                    "device",
                    black_box(mode),
                ))
                .unwrap();
            });
        }
        "enumerate" => run_work(iterations, || {
            black_box(enumerate_output_devices());
        }),
        "rate_capabilities" => {
            let pcm = open_pcm("null").unwrap();
            let hw = HwParams::any(&pcm).unwrap();
            run_work(iterations, || {
                black_box(sample_rate_capabilities(black_box(&hw))).unwrap();
            });
        }
        "probe_null" => run_work(iterations, || {
            black_box(probe_output_device(black_box("null"))).unwrap();
        }),
        "clock_read_mono" | "clock_read_raw" => {
            let clock = if workload == "clock_read_mono" {
                AlsaHostClock::Monotonic
            } else {
                AlsaHostClock::MonotonicRaw
            };
            run_work(iterations, || {
                black_box(current_clock_nanos(black_box(clock))).unwrap();
            });
        }
        "clock_map_recent" | "clock_map_future" | "clock_map_edges" => {
            let mut counter = 0_u64;
            let samples = [
                (0, 0, u64::MAX),
                (0, u64::MAX, 0),
                (u64::MAX, 0, u64::MAX),
                (u64::MAX, u64::MAX, 0),
                (5_000, 2_000, 1_250),
                (5_000, 2_000, 2_750),
                (0, 0, 0),
                (1, 1, 1),
            ];
            match workload.as_str() {
                "clock_map_edges" => run_work(iterations, || {
                    let (host, clock, target) = black_box(samples[counter as usize & 7]);
                    counter = counter.wrapping_add(1);
                    black_box(host_nanos_from_clock(
                        target,
                        ClockSample {
                            host_nanos: host,
                            clock_nanos: clock,
                        },
                    ));
                }),
                _ => {
                    let future = workload == "clock_map_future";
                    run_work(iterations, || {
                        let offset = counter & 1023;
                        let target = if future {
                            1_000_000_000_000 + offset
                        } else {
                            1_000_000_000_000 - offset
                        };
                        counter = counter.wrapping_add(1);
                        black_box(host_nanos_from_clock(
                            black_box(target),
                            black_box(ClockSample {
                                host_nanos: 1_000_000_000,
                                clock_nanos: 1_000_000_000_000,
                            }),
                        ));
                    });
                }
            }
        }
        "timing_48000" | "timing_44100" | "timing_96000" => {
            let rate = match workload.as_str() {
                "timing_48000" => 48_000,
                "timing_44100" => 44_100,
                _ => 96_000,
            };
            let pcm = open_pcm("null").unwrap();
            let actual =
                configure_pcm(&pcm, rate, 2, AlsaAccessMode::Shared, 240, Some(480)).unwrap();
            let mut health = AlsaClockHealth::new();
            run_work(iterations, || {
                black_box(playback_status_timing(
                    &pcm,
                    black_box(rate),
                    actual.sample_rate_reciprocal,
                    actual.sample_rate_shift,
                    actual.host_clock,
                    &mut health,
                ));
            });
        }
        "frames_48000" | "frames_44100" | "frames_other" => {
            let rate = match workload.as_str() {
                "frames_48000" => 48_000,
                "frames_44100" => 44_100,
                _ => 96_000,
            };
            let (reciprocal, shift) = sample_rate_scale(rate);
            let mut frame = 0u32;
            run_work(iterations, || {
                frame = frame.wrapping_add(239);
                black_box(frames_to_nanos(
                    black_box(rate),
                    black_box(frame),
                    black_box(reciprocal),
                    black_box(shift),
                ));
            });
        }
        "frames_mixed" => {
            let mut frame = 0u32;
            let rates = [48_000, 44_100, 96_000, 192_000];
            let scales = rates.map(sample_rate_scale);
            run_work(iterations, || {
                let index = frame as usize & 3;
                let rate = rates[index];
                frame = frame.wrapping_add(239);
                black_box(frames_to_nanos(
                    black_box(rate),
                    black_box(frame),
                    black_box(scales[index].0),
                    black_box(scales[index].1),
                ));
            });
        }
        _ => panic!("unknown workload {workload}"),
    }
}

fn run_work(iterations: usize, mut work: impl FnMut()) {
    work();
    COUNTING.store(true, Ordering::Relaxed);
    work();
    COUNTING.store(false, Ordering::Relaxed);
    println!(
        "allocations={} reallocations={} requested_bytes={}",
        ALLOCS.load(Ordering::Relaxed),
        REALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed)
    );
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    println!(
        "operations={iterations} elapsed_ns={}",
        start.elapsed().as_nanos()
    );
}
