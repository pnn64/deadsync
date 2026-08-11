use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use deadsync_audio::f32_to_i16;
use deadsync_audio_decode::resample::write_resampler_output;

const FRAMES: usize = 4_096;
const CHANNELS: usize = 2;
const SAMPLES: usize = FRAMES * CHANNELS;
const WARMUP_RUNS: usize = 1_000;
const MEASURE_RUNS: usize = 20_000;
const SFX_COLLECT_RUNS: usize = 2_000;
const CORRELATION_RUNS: usize = 2_000;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.frees.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplies the allocation's original pointer and layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
    runs: usize,
    units: usize,
}

fn callback_old(sample: f32) -> i16 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample >= 1.0 {
        i16::MAX
    } else if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * 32_768.0) as i16
    }
}

fn resampler_sample_old(sample: f32) -> i16 {
    (sample * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

fn write_resampler_output_old(
    out: &[Vec<f32>],
    produced_frames: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    let produced_frames = produced_frames.min(out[0].len()).min(out[1].len());
    out_tmp.resize(produced_frames * 2, 0);
    for frame in 0..produced_frames {
        let base = frame * 2;
        out_tmp[base] = resampler_sample_old(out[0][frame]);
        out_tmp[base + 1] = resampler_sample_old(out[1][frame]);
    }
    produced_frames
}

fn measure_convert(input: &[f32], convert: fn(f32) -> i16) -> BenchResult {
    let mut out = vec![0i16; input.len()];
    let mut checksum = 0u64;
    for _ in 0..WARMUP_RUNS {
        for (dst, &src) in out.iter_mut().zip(black_box(input)) {
            *dst = convert(src);
        }
        black_box(&out);
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for run in 0..MEASURE_RUNS {
        for (dst, &src) in out.iter_mut().zip(black_box(input)) {
            *dst = convert(src);
        }
        checksum = checksum.rotate_left(5) ^ out[run % out.len()] as u16 as u64;
        black_box(&out);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        runs: MEASURE_RUNS,
        units: input.len() * MEASURE_RUNS,
    }
}

fn measure_resampler_output(
    input: &[Vec<f32>],
    write: fn(&[Vec<f32>], usize, &mut Vec<i16>) -> usize,
) -> BenchResult {
    let mut out = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..WARMUP_RUNS {
        black_box(write(black_box(input), FRAMES, &mut out));
        black_box(&out);
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for run in 0..MEASURE_RUNS {
        black_box(write(black_box(input), FRAMES, &mut out));
        checksum = checksum.rotate_left(5) ^ out[run % out.len()] as u16 as u64;
        black_box(&out);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        runs: MEASURE_RUNS,
        units: SAMPLES * MEASURE_RUNS,
    }
}

fn write_resampler_output_candidate(
    out: &[Vec<f32>],
    produced_frames: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    write_resampler_output(out, produced_frames, 2, out_tmp)
}

fn measure_sfx_collect(packet: &[i16], samples: usize, reserve: bool) -> BenchResult {
    let run = || {
        let mut out = if reserve {
            Vec::with_capacity(samples)
        } else {
            Vec::new()
        };
        while out.len() < samples {
            let copy = packet.len().min(samples - out.len());
            out.extend_from_slice(&packet[..copy]);
        }
        black_box(out[out.len() - 1]) as u16 as u64
    };
    for _ in 0..100 {
        black_box(run());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SFX_COLLECT_RUNS {
        checksum = checksum.rotate_left(5) ^ run();
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        runs: SFX_COLLECT_RUNS,
        units: samples * SFX_COLLECT_RUNS,
    }
}

fn closest_match_old(buffer: &[f32], correlate: &[f32]) -> usize {
    let distance = buffer.len() - correlate.len();
    let mut best_offset = 0usize;
    let mut best_score = f32::INFINITY;
    for i in 0..=distance {
        let mut score = 0.0f32;
        let frames = &buffer[i..i + correlate.len()];
        for j in 0..correlate.len() {
            score += (frames[j] - correlate[j]).abs();
            if score >= best_score {
                break;
            }
        }
        if score < best_score {
            best_score = score;
            best_offset = i;
        }
    }
    best_offset
}

fn closest_match_chunked(buffer: &[f32], correlate: &[f32]) -> usize {
    let distance = buffer.len() - correlate.len();
    let mut best_offset = 0usize;
    let mut best_score = f32::INFINITY;
    for i in 0..=distance {
        let frames = &buffer[i..i + correlate.len()];
        let mut score = 0.0f32;
        let mut j = 0usize;
        while j + 8 <= correlate.len() {
            score += (frames[j] - correlate[j]).abs();
            score += (frames[j + 1] - correlate[j + 1]).abs();
            score += (frames[j + 2] - correlate[j + 2]).abs();
            score += (frames[j + 3] - correlate[j + 3]).abs();
            score += (frames[j + 4] - correlate[j + 4]).abs();
            score += (frames[j + 5] - correlate[j + 5]).abs();
            score += (frames[j + 6] - correlate[j + 6]).abs();
            score += (frames[j + 7] - correlate[j + 7]).abs();
            j += 8;
            if score >= best_score {
                break;
            }
        }
        if score < best_score {
            while j < correlate.len() {
                score += (frames[j] - correlate[j]).abs();
                j += 1;
            }
        }
        if score < best_score {
            best_score = score;
            best_offset = i;
        }
    }
    best_offset
}

fn measure_correlation(
    buffer: &[f32],
    correlate: &[f32],
    find: fn(&[f32], &[f32]) -> usize,
) -> BenchResult {
    let expected = find(buffer, correlate);
    for _ in 0..100 {
        assert_eq!(
            black_box(find(black_box(buffer), black_box(correlate))),
            expected
        );
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..CORRELATION_RUNS {
        checksum = checksum.rotate_left(5)
            ^ black_box(find(black_box(buffer), black_box(correlate))) as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        runs: CORRELATION_RUNS,
        units: CORRELATION_RUNS,
    }
}

fn print_result(label: &str, unit: &str, result: &BenchResult) {
    let units = result.units as f64;
    println!(
        "  {label:<9} {:>9.3} ns/{unit}  {:>9.3} cycles/{unit}  {:>9.3} M{unit}/s  \
         {:>5.3} alloc/realloc/free per run  {:>6.1} B/run",
        result.elapsed.as_secs_f64() * 1.0e9 / units,
        result.cycles as f64 / units,
        units / result.elapsed.as_secs_f64() / 1.0e6,
        (result.alloc.allocs + result.alloc.reallocs + result.alloc.frees) as f64
            / result.runs as f64,
        result.alloc.bytes as f64 / result.runs as f64,
    );
}

fn print_pair(name: &str, unit: &str, old: &BenchResult, candidate: &BenchResult) {
    assert_eq!(old.checksum, candidate.checksum);
    assert_eq!(old.units, candidate.units);
    println!("{name}");
    print_result("old", unit, old);
    print_result("candidate", unit, candidate);
    println!(
        "  speedup {:.2}x | cycle reduction {:.1}%",
        old.elapsed.as_secs_f64() / candidate.elapsed.as_secs_f64(),
        100.0 * (1.0 - candidate.cycles as f64 / old.cycles as f64),
    );
}

fn main() {
    let float_input = (0..SAMPLES)
        .map(|index| {
            let unit = (index.wrapping_mul(1_103_515_245) as u32) as f32 / u32::MAX as f32;
            unit.mul_add(3.0, -1.5)
        })
        .collect::<Vec<_>>();
    println!("audio/resampling hot paths");
    print_pair(
        "callback f32 -> i16",
        "sample",
        &measure_convert(&float_input, callback_old),
        &measure_convert(&float_input, f32_to_i16),
    );

    let planar_output = vec![
        float_input.iter().copied().step_by(2).collect::<Vec<_>>(),
        float_input
            .iter()
            .copied()
            .skip(1)
            .step_by(2)
            .collect::<Vec<_>>(),
    ];
    print_pair(
        "resampler output f32 -> interleaved i16",
        "sample",
        &measure_resampler_output(&planar_output, write_resampler_output_old),
        &measure_resampler_output(&planar_output, write_resampler_output_candidate),
    );

    let packet = (0..1_152 * CHANNELS)
        .map(|index| index.wrapping_mul(25_173) as i16)
        .collect::<Vec<_>>();
    let sfx_samples = 48_000 * CHANNELS;
    print_pair(
        "one-second stereo SFX collection",
        "sample",
        &measure_sfx_collect(&packet, sfx_samples, false),
        &measure_sfx_collect(&packet, sfx_samples, true),
    );

    let correlate = (0..360)
        .map(|index| ((index as f32 * 0.071).sin() * 0.7) + ((index % 17) as f32 * 0.001))
        .collect::<Vec<_>>();
    let mut search = (0..720)
        .map(|index| ((index as f32 * 0.069).sin() * 0.7) + ((index % 13) as f32 * 0.001))
        .collect::<Vec<_>>();
    search[211..211 + correlate.len()].copy_from_slice(&correlate);
    print_pair(
        "SOLA L1 correlation",
        "search",
        &measure_correlation(&search, &correlate, closest_match_old),
        &measure_correlation(&search, &correlate, closest_match_chunked),
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
