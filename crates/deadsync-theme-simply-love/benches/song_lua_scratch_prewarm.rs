use deadsync_theme_simply_love::screens::gameplay::SongLuaScratchPrewarmBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 256;
const MAIN_OVERLAYS: usize = 64;
const BACKGROUND_LAYERS: [usize; 2] = [32, 96];
const FOREGROUND_LAYERS: [usize; 1] = [48];

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls while measurement is active.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
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
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    ns_per_song: f64,
    cycles_per_song: Option<f64>,
    allocated: AllocSnapshot,
    checksum: usize,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn cases(prewarmed: bool) -> Vec<SongLuaScratchPrewarmBenchmark> {
    (0..SONGS)
        .map(|_| {
            if prewarmed {
                SongLuaScratchPrewarmBenchmark::prewarmed(
                    MAIN_OVERLAYS,
                    &BACKGROUND_LAYERS,
                    &FOREGROUND_LAYERS,
                )
            } else {
                SongLuaScratchPrewarmBenchmark::cold(
                    MAIN_OVERLAYS,
                    &BACKGROUND_LAYERS,
                    &FOREGROUND_LAYERS,
                )
            }
        })
        .collect()
}

fn measure(prewarmed: bool) -> BenchResult {
    let mut warmup = cases(prewarmed);
    let mut timed = cases(prewarmed);
    let mut allocated = cases(prewarmed);
    for song in &mut warmup {
        black_box(song.opening_frame());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0usize;
    for song in &mut timed {
        checksum = checksum.wrapping_add(black_box(song.opening_frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0usize;
    for song in &mut allocated {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(song.opening_frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocation_delta = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_song: elapsed.as_secs_f64() * 1_000_000_000.0 / SONGS as f64,
        cycles_per_song: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / SONGS as f64),
        allocated: allocation_delta,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let songs = SONGS as f64;
    println!(
        "{label:<18} {:>10.2} ns/song  {:>10.2} cycles/song  {:>7.2} Ksong/s  \
         {:>5.2} allocs/song  {:>8.1} bytes/song  {:>5.2} reallocs/song",
        result.ns_per_song,
        result.cycles_per_song.unwrap_or(f64::NAN),
        1_000_000.0 / result.ns_per_song,
        result.allocated.allocs as f64 / songs,
        result.allocated.bytes as f64 / songs,
        result.allocated.reallocs as f64 / songs,
    );
}

fn main() {
    let cold = measure(false);
    let prewarmed = measure(true);
    assert_eq!(cold.checksum, prewarmed.checksum);

    let scratch = SongLuaScratchPrewarmBenchmark::prewarmed(
        MAIN_OVERLAYS,
        &BACKGROUND_LAYERS,
        &FOREGROUND_LAYERS,
    );
    println!(
        "SongLua opening-frame scratch prewarm ({MAIN_OVERLAYS} main, {:?} BG, {:?} FG)",
        BACKGROUND_LAYERS, FOREGROUND_LAYERS
    );
    print_result("cold first frame", &cold);
    print_result("song-prewarmed", &prewarmed);
    println!(
        "prewarmed scratch storage: {} KiB/song",
        scratch.storage_bytes() / 1024
    );
}
