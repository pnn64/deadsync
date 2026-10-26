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
        let name = if layout == "bad_name" {
            format!("Fixture card 音楽, Device {}", index % 4)
        } else {
            "Fixture card 音楽, Fixture playback".to_owned()
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
    let delay: i64 = std::env::var("ALSA_FIXTURE_DELAY")
        .unwrap()
        .parse()
        .unwrap();
    let frames = delay.max(0) as u32;
    let mut health = AlsaClockHealth::new();
    let timing = playback_status_timing(&pcm, 48_000, actual.host_clock, &mut health);
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
            let mut frame = 0u32;
            run_work(iterations, || {
                frame = frame.wrapping_add(239);
                black_box(frames_to_nanos(black_box(rate), black_box(frame)));
            });
        }
        "frames_mixed" => {
            let mut frame = 0u32;
            run_work(iterations, || {
                let rate = [48_000, 44_100, 96_000, 192_000][frame as usize & 3];
                frame = frame.wrapping_add(239);
                black_box(frames_to_nanos(black_box(rate), black_box(frame)));
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
