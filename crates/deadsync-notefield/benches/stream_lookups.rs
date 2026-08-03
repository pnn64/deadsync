use deadsync_notefield::{
    BrokenRunLookup, StreamProgressLookup, benchmark_broken_run_segment_legacy,
    zmod_stream_prog_completion_for_beat,
};
use deadsync_rules::stream::StreamSegment;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const STREAM_SEGMENTS: usize = 8_192;
const STREAM_WARMUP_FRAMES: usize = 256;
const STREAM_MEASURE_FRAMES: usize = 10_000;
const BROKEN_SEGMENTS: usize = 512;
const BROKEN_WARMUP_FRAMES: usize = 16;
const BROKEN_MEASURE_FRAMES: usize = 2_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller's unchanged
// pointer and layout; relaxed atomics only observe successful allocation work.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this allocation layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplied a live pointer/layout pair.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplied the live pointer and its current layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    frames: usize,
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let segments = alternating_segments(STREAM_SEGMENTS);
    let total_stream = segments
        .iter()
        .filter(|segment| !segment.is_break)
        .map(|segment| (segment.end - segment.start) as f64)
        .sum();
    let progress = StreamProgressLookup::new(&segments);
    let old_progress = run(STREAM_WARMUP_FRAMES, STREAM_MEASURE_FRAMES, |frame| {
        let beat = progress_beat(frame, STREAM_SEGMENTS);
        zmod_stream_prog_completion_for_beat(total_stream, &segments, beat)
            .unwrap_or_default()
            .to_bits()
    });
    let new_progress = run(STREAM_WARMUP_FRAMES, STREAM_MEASURE_FRAMES, |frame| {
        let beat = progress_beat(frame, STREAM_SEGMENTS);
        progress
            .completion_for_beat(total_stream, beat)
            .unwrap_or_default()
            .to_bits()
    });
    assert_eq!(old_progress.checksum, new_progress.checksum);

    let broken_segments = alternating_segments(BROKEN_SEGMENTS);
    let broken = BrokenRunLookup::new(&broken_segments);
    let after_chart = broken_segments
        .last()
        .map_or(0.0, |segment| segment.end as f32 + 1.0);
    let old_broken = run(BROKEN_WARMUP_FRAMES, BROKEN_MEASURE_FRAMES, |frame| {
        broken_checksum(benchmark_broken_run_segment_legacy(
            &broken_segments,
            if frame % 257 == 0 { 0.0 } else { after_chart },
        ))
    });
    let new_broken = run(BROKEN_WARMUP_FRAMES, BROKEN_MEASURE_FRAMES, |frame| {
        broken_checksum(broken.segment(if frame % 257 == 0 { 0.0 } else { after_chart }))
    });
    assert_eq!(old_broken.checksum, new_broken.checksum);

    println!("gameplay stream lookup benchmark");
    println!("  StreamProg fixture: {STREAM_SEGMENTS} alternating segments");
    println!("  broken-run fixture: {BROKEN_SEGMENTS} alternating segments");
    print_pair(
        "StreamProg full scan",
        "prefix lookup",
        &old_progress,
        &new_progress,
    );
    println!("  lookup storage: {} bytes", progress.storage_bytes());
    print_pair("broken-run rescan", "span lookup", &old_broken, &new_broken);
    println!("  lookup storage: {} bytes", broken.storage_bytes());
}

fn alternating_segments(segment_count: usize) -> Vec<StreamSegment> {
    let mut start = 0usize;
    (0..segment_count)
        .map(|index| {
            let is_break = index % 2 == 1;
            let len = if is_break { 2 } else { 8 };
            let segment = StreamSegment {
                start,
                end: start + len,
                is_break,
            };
            start += len;
            segment
        })
        .collect()
}

fn progress_beat(frame: usize, segment_count: usize) -> f32 {
    ((frame * 13) % (segment_count * 5)) as f32 * 4.0
}

fn broken_checksum(value: Option<(usize, i32, bool)>) -> u64 {
    value.map_or(u64::MAX, |(index, end, broken)| {
        (index as u64).rotate_left(17) ^ end as u32 as u64 ^ u64::from(broken)
    })
}

fn run(
    warmup_frames: usize,
    measure_frames: usize,
    mut frame: impl FnMut(usize) -> u64,
) -> BenchResult {
    for index in 0..warmup_frames {
        black_box(frame(index));
    }
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for index in warmup_frames..warmup_frames + measure_frames {
        checksum = checksum.rotate_left(7) ^ black_box(frame(index));
    }
    BenchResult {
        frames: measure_frames,
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn print_pair(old_name: &str, new_name: &str, old: &BenchResult, new: &BenchResult) {
    print_result(old_name, old);
    print_result(new_name, new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = result.frames as f64;
    println!(
        "  {name:<20} {:>10.1} ns/frame {:>10.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "  {:<20} allocs={} reallocs={} frees={} bytes={}",
        "memory",
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.deallocs,
        result.alloc.bytes,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
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
