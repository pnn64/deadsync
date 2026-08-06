use deadsync_gameplay::{
    GameplayMiniIndicatorOptions, mini_indicator_needs_stream_data,
    zmod_stream_totals_for_densities,
};
use deadsync_rules::stream::measure_densities;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CASES: usize = 2_000;

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

// SAFETY: every operation delegates to `System` with the original allocator
// arguments; relaxed atomics only observe allocation churn.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.frees.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
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
    max_sample: Duration,
    allocated: AllocSnapshot,
    checksum: usize,
}

fn chart_notes() -> Vec<u8> {
    let mut notes = Vec::with_capacity(512 * 64 * 5);
    for measure in 0..512 {
        for row in 0..64 {
            let line = if (row + measure) % 4 == 0 {
                b"1000\n"
            } else {
                b"0000\n"
            };
            notes.extend_from_slice(line);
        }
        notes.extend_from_slice(if measure == 511 { b";\n" } else { b",\n" });
    }
    notes
}

fn legacy_pacemaker_prep(notes: &[u8]) -> usize {
    let densities = measure_densities(notes, 4);
    let (segments, stream, breaks) = zmod_stream_totals_for_densities(&densities, true);
    segments
        .len()
        .wrapping_add(stream.to_bits() as usize)
        .wrapping_add(breaks.to_bits() as usize)
}

fn optimized_pacemaker_prep(notes: &[u8]) -> usize {
    let options = GameplayMiniIndicatorOptions {
        pacemaker: true,
        ..GameplayMiniIndicatorOptions::default()
    };
    if !mini_indicator_needs_stream_data(options) {
        return 0;
    }
    legacy_pacemaker_prep(notes)
}

fn measure(notes: &[u8], mut prepare: impl FnMut(&[u8]) -> usize) -> BenchResult {
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut max_sample = Duration::ZERO;
    let mut checksum = 0usize;
    for _ in 0..CASES {
        let sample_started = Instant::now();
        checksum = checksum.rotate_left(7) ^ black_box(prepare(black_box(notes)));
        max_sample = max_sample.max(sample_started.elapsed());
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        max_sample,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let us_per_case = result.elapsed.as_secs_f64() * 1e6 / CASES as f64;
    let cycles_per_case = result.cycles as f64 / CASES as f64;
    let throughput = CASES as f64 / result.elapsed.as_secs_f64();
    println!(
        "{label:20} {us_per_case:9.2} us/case  {cycles_per_case:11.0} cycles/case  \
         {throughput:9.0} cases/s  max {max_us:8.2} us  alloc {allocs}  \
         realloc {reallocs}  free {frees}  bytes {bytes}",
        max_us = result.max_sample.as_secs_f64() * 1e6,
        allocs = result.allocated.allocs,
        reallocs = result.allocated.reallocs,
        frees = result.allocated.frees,
        bytes = result.allocated.bytes,
    );
}

fn main() {
    let notes = chart_notes();
    assert_ne!(legacy_pacemaker_prep(&notes), 0);
    assert_eq!(optimized_pacemaker_prep(&notes), 0);

    let legacy = measure(&notes, legacy_pacemaker_prep);
    let optimized = measure(&notes, optimized_pacemaker_prep);
    black_box((legacy.checksum, optimized.checksum));

    println!("pacemaker song-entry stream preparation ({CASES} chart loads)");
    print_result("legacy any-mode prep", &legacy);
    print_result("consumer-gated prep", &optimized);
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
