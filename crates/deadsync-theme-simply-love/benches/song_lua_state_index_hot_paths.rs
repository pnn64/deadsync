use deadsync_theme_simply_love::screens::gameplay::{
    SongLuaAftSpriteIndexBenchmark, SongLuaCaptureStateBenchmark, SongLuaUpdateSourceBenchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    calls: AtomicU64,
    frees: AtomicU64,
    churn: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            calls: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            calls: self.calls.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            churn: self.churn.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator calls delegate unchanged to `System`; relaxed counters
// only observe successful calls while this single-threaded benchmark is gated.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    calls: u64,
    frees: u64,
    churn: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            calls: self.calls - before.calls,
            frees: self.frees - before.frees,
            churn: self.churn - before.churn,
        }
    }
}

struct ResultRow {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut frame: impl FnMut() -> u64) -> ResultRow {
    for _ in 0..(iterations / 20).max(20) {
        black_box(frame());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        black_box(frame());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before);
    ResultRow {
        ns: elapsed.as_secs_f64() * 1e9 / iterations as f64,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        alloc,
        checksum,
    }
}

fn run(title: &str, iterations: usize, old: impl FnMut() -> u64, new: impl FnMut() -> u64) {
    let old = measure(iterations, old);
    let new = measure(iterations, new);
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(
        (old.alloc.calls, old.alloc.frees, old.alloc.churn),
        (0, 0, 0)
    );
    assert_eq!(
        (new.alloc.calls, new.alloc.frees, new.alloc.churn),
        (0, 0, 0)
    );
    println!("\n{title}");
    print_row("old", iterations, &old);
    print_row("new", iterations, &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% throughput",
        change(old.ns, new.ns),
        change(
            old.cycles.unwrap_or(f64::NAN),
            new.cycles.unwrap_or(f64::NAN)
        ),
        change(1.0 / old.ns, 1.0 / new.ns),
    );
}

fn print_row(label: &str, iterations: usize, row: &ResultRow) {
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>5.2} alloc/frame  {:>5.2} free/frame  {:>8.1} churn B/frame",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        1_000.0 / row.ns,
        row.alloc.calls as f64 / iterations as f64,
        row.alloc.frees as f64 / iterations as f64,
        row.alloc.churn as f64 / iterations as f64,
    );
}

fn change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
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

fn main() {
    let old_source = SongLuaUpdateSourceBenchmark::new(513);
    let new_source = SongLuaUpdateSourceBenchmark::new(513);
    run(
        "update source lookup (513 actors)",
        2_000_000,
        || old_source.reference_frame(),
        || new_source.current_frame(),
    );

    let mut old_capture = SongLuaCaptureStateBenchmark::new(513, 31);
    let mut new_capture = SongLuaCaptureStateBenchmark::new(513, 31);
    run(
        "AFT capture state refresh (31 of 513 actors)",
        100_000,
        || old_capture.reference_frame(),
        || new_capture.current_frame(),
    );

    let mut old_sprites = SongLuaAftSpriteIndexBenchmark::new(513, 8);
    let mut new_sprites = SongLuaAftSpriteIndexBenchmark::new(513, 8);
    run(
        "AFT sprite traversal (8 of 513 actors)",
        2_000_000,
        || old_sprites.reference_frame(),
        || new_sprites.current_frame(),
    );
}
