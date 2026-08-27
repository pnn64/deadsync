use deadsync_audio_decode::resample::{
    PlanarAccum, append_channel_mapped_i16, bench_support::take_seek_buffer,
    write_channel_mapped_i16,
};
use deadsync_audio_stream::decode_bench_support::planar_window_checksum;
use smallvec::SmallVec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLE_BATCHES: usize = 32;
const PLANAR_CHANNELS: usize = 6;
const PLANAR_FRAMES: usize = 256;
const PLANAR_OPS: usize = 500_000;
const SEEK_PACKET_SAMPLES: usize = 4_096;
const SEEK_OPS: usize = 200_000;
const MAP_IN_CHANNELS: usize = 6;
const MAP_OUT_CHANNELS: usize = 2;
const MAP_PACKET_FRAMES: usize = 1_024;
const MAP_PACKETS: usize = 8;
const MAP_OPS: usize = 20_000;
const ALLOC_OPS: usize = 128;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller-provided
// pointer and layout. Relaxed counters only observe calls while gated on.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_unit: f64,
    cycles_per_unit: Option<f64>,
    units_per_second: f64,
    worst_op_ns: f64,
    allocated: AllocSnapshot,
    alloc_ops: usize,
    checksum: u64,
}

fn measure(
    ops: usize,
    units_per_op: usize,
    alloc_ops: usize,
    mut operation: impl FnMut() -> u64,
) -> BenchResult {
    for _ in 0..(ops / 20).max(1) {
        black_box(operation());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_op_ns = 0.0f64;
    let ops_per_batch = ops / SAMPLE_BATCHES;
    for _ in 0..SAMPLE_BATCHES {
        let sample_started = Instant::now();
        for _ in 0..ops_per_batch {
            checksum = checksum.rotate_left(5) ^ black_box(operation());
        }
        worst_op_ns = worst_op_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / ops_per_batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    let measured_ops = ops_per_batch * SAMPLE_BATCHES;

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut alloc_checksum = 0u64;
    for _ in 0..alloc_ops {
        alloc_checksum = alloc_checksum.rotate_left(5) ^ black_box(operation());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(alloc_checksum);

    let units = measured_ops * units_per_op;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_unit: seconds * 1_000_000_000.0 / units as f64,
        cycles_per_unit: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / units as f64),
        units_per_second: units as f64 / seconds,
        worst_op_ns,
        allocated,
        alloc_ops,
        checksum,
    }
}

fn print_pair(title: &str, unit: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", unit, old);
    print_result("new", unit, new);
}

fn print_result(label: &str, unit: &str, result: &BenchResult) {
    let alloc_ops = result.alloc_ops as f64;
    println!(
        "  {label:<3} {:>10.3} ns/{unit}  {:>9.3} cycles/{unit}  {:>9.3} M{unit}/s  \
         {:>10.3} worst ns/op  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  \
         {:>10.1} churn B/op",
        result.ns_per_unit,
        result.cycles_per_unit.unwrap_or(f64::NAN),
        result.units_per_second / 1_000_000.0,
        result.worst_op_ns,
        result.allocated.allocs as f64 / alloc_ops,
        result.allocated.reallocs as f64 / alloc_ops,
        result.allocated.frees as f64 / alloc_ops,
        result.allocated.churn_bytes() as f64 / alloc_ops,
    );
}

fn old_planar_window_checksum(planar: &PlanarAccum, frames: usize) -> u64 {
    let start = planar.start_frame;
    let end = start + frames;
    let mut slices = SmallVec::<[&[f32]; 2]>::with_capacity(planar.channels.len());
    slices.extend(planar.channels.iter().map(|channel| &channel[start..end]));
    slices
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (channel, samples)| {
            checksum
                .rotate_left(5)
                .wrapping_add(u64::from(samples[channel % samples.len()].to_bits()))
                .wrapping_add(u64::from(samples[samples.len() - 1].to_bits()))
        })
}

#[allow(clippy::slow_vector_initialization)] // Retained pre-change path for comparison.
fn reset_seek_old(slot: &mut Option<Vec<i16>>, marker: i16) -> u64 {
    drop(slot.take());
    let mut scratch = Vec::new();
    scratch.resize(SEEK_PACKET_SAMPLES, 0);
    scratch[0] = marker;
    black_box(&mut scratch);
    let checksum = u64::from(scratch[0] as u16) ^ scratch.len() as u64;
    *slot = Some(scratch);
    checksum
}

fn reset_seek_new(slot: &mut Option<Vec<i16>>, marker: i16) -> u64 {
    let mut scratch = take_seek_buffer(slot);
    scratch.resize(SEEK_PACKET_SAMPLES, 0);
    scratch[0] = marker;
    black_box(&mut scratch);
    let checksum = u64::from(scratch[0] as u16) ^ scratch.len() as u64;
    *slot = Some(scratch);
    checksum
}

fn collect_mapped_old(input: &[i16]) -> u64 {
    let output_samples = MAP_PACKET_FRAMES * MAP_PACKETS * MAP_OUT_CHANNELS;
    let mut output = Vec::with_capacity(output_samples);
    let mut packet = Vec::new();
    for _ in 0..MAP_PACKETS {
        write_channel_mapped_i16(input, MAP_IN_CHANNELS, MAP_OUT_CHANNELS, &mut packet);
        output.extend_from_slice(&packet);
    }
    checksum_i16(&output)
}

fn collect_mapped_new(input: &[i16]) -> u64 {
    let output_samples = MAP_PACKET_FRAMES * MAP_PACKETS * MAP_OUT_CHANNELS;
    let mut output = Vec::with_capacity(output_samples);
    for _ in 0..MAP_PACKETS {
        append_channel_mapped_i16(input, MAP_IN_CHANNELS, MAP_OUT_CHANNELS, &mut output);
    }
    checksum_i16(&output)
}

fn checksum_i16(samples: &[i16]) -> u64 {
    samples.iter().step_by(257).fold(0u64, |checksum, sample| {
        checksum.rotate_left(5) ^ u64::from(*sample as u16)
    })
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    let planar = PlanarAccum {
        channels: (0..PLANAR_CHANNELS)
            .map(|channel| {
                (0..PLANAR_FRAMES + 17)
                    .map(|frame| ((channel * PLANAR_FRAMES + frame) as f32) * 0.001)
                    .collect()
            })
            .collect(),
        start_frame: 11,
    };
    assert_eq!(
        old_planar_window_checksum(&planar, PLANAR_FRAMES),
        planar_window_checksum(&planar, PLANAR_FRAMES)
    );
    let old_planar = measure(PLANAR_OPS, PLANAR_CHANNELS, ALLOC_OPS, || {
        old_planar_window_checksum(black_box(&planar), PLANAR_FRAMES)
    });
    let new_planar = measure(PLANAR_OPS, PLANAR_CHANNELS, ALLOC_OPS, || {
        planar_window_checksum(black_box(&planar), PLANAR_FRAMES)
    });
    print_pair(
        "six-channel resampler input views",
        "view",
        &old_planar,
        &new_planar,
    );

    let mut old_slot = Some(vec![7; SEEK_PACKET_SAMPLES]);
    let mut old_marker = 0i16;
    let old_seek = measure(SEEK_OPS, 1, ALLOC_OPS, || {
        old_marker = old_marker.wrapping_add(1);
        reset_seek_old(&mut old_slot, old_marker)
    });
    let mut new_slot = Some(vec![7; SEEK_PACKET_SAMPLES]);
    let mut new_marker = 0i16;
    let new_seek = measure(SEEK_OPS, 1, ALLOC_OPS, || {
        new_marker = new_marker.wrapping_add(1);
        reset_seek_new(&mut new_slot, new_marker)
    });
    print_pair("decoder seek scratch reset", "seek", &old_seek, &new_seek);

    let map_input = (0..MAP_PACKET_FRAMES * MAP_IN_CHANNELS)
        .map(|index| index.wrapping_mul(19_997) as i16)
        .collect::<Vec<_>>();
    assert_eq!(
        collect_mapped_old(&map_input),
        collect_mapped_new(&map_input)
    );
    let old_map = measure(MAP_OPS, MAP_PACKET_FRAMES * MAP_PACKETS, ALLOC_OPS, || {
        collect_mapped_old(black_box(&map_input))
    });
    let new_map = measure(MAP_OPS, MAP_PACKET_FRAMES * MAP_PACKETS, ALLOC_OPS, || {
        collect_mapped_new(black_box(&map_input))
    });
    print_pair(
        "eight-packet surround SFX channel map",
        "frame",
        &old_map,
        &new_map,
    );
}
