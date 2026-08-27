use deadsync_song_lua::{
    SongLuaEaseTarget, SongLuaEaseWindow, SongLuaSpanMode, SongLuaTimeUnit,
    extend_runtime_mod_sustains,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    churn_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            churn_bytes: self.churn_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls during single-threaded runs.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
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
            self.churn_bytes
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
    churn_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            churn_bytes: self.churn_bytes - before.churn_bytes,
        }
    }
}

struct BenchResult {
    ns_per_compile: f64,
    cycles_per_compile: Option<f64>,
    throughput: f64,
    allocations: AllocSnapshot,
    checksum: u64,
}

fn fixture(count: usize) -> Vec<SongLuaEaseWindow> {
    (0..count)
        .map(|index| SongLuaEaseWindow {
            unit: SongLuaTimeUnit::Beat,
            start: ((index * 73) % count) as f32 * 0.25,
            limit: 0.125 + (index % 5) as f32 * 0.0625,
            span_mode: SongLuaSpanMode::Len,
            from: 0.0,
            to: 1.0,
            target: if index % 3 == 0 {
                SongLuaEaseTarget::PlayerRotationZ
            } else {
                SongLuaEaseTarget::Mod(format!("mod{}", index % 11))
            },
            easing: Some("linear".to_string()),
            player: Some((index % 2 + 1) as u8),
            sustain: None,
            opt1: None,
            opt2: None,
        })
        .collect()
}

fn reset(windows: &mut [SongLuaEaseWindow]) {
    for window in windows {
        window.sustain = None;
    }
}

fn extend_reference(windows: &mut [SongLuaEaseWindow]) {
    const DEFAULT_SUSTAIN_BEATS: f32 = 1_000_000.0;
    const SAME_TICK_EPSILON: f32 = 0.001;

    for index in 0..windows.len() {
        let end = windows[index].start + windows[index].limit;
        let next_start = windows
            .iter()
            .enumerate()
            .filter_map(|(other_index, other)| {
                (other_index != index
                    && other.player == windows[index].player
                    && other.target == windows[index].target
                    && other.start > windows[index].start + SAME_TICK_EPSILON)
                    .then_some(other.start)
            })
            .min_by(f32::total_cmp)
            .unwrap_or(DEFAULT_SUSTAIN_BEATS);
        if next_start > end + SAME_TICK_EPSILON {
            windows[index].sustain = Some(next_start - end);
        }
    }
}

fn checksum(windows: &[SongLuaEaseWindow]) -> u64 {
    windows.iter().fold(0u64, |sum, window| {
        sum.wrapping_mul(16_777_619)
            .wrapping_add(window.sustain.unwrap_or_default().to_bits() as u64)
    })
}

fn measure(
    iterations: usize,
    windows: &mut [SongLuaEaseWindow],
    mut extend: impl FnMut(&mut [SongLuaEaseWindow]),
) -> BenchResult {
    for _ in 0..10 {
        reset(windows);
        extend(black_box(windows));
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut total = 0u64;
    for _ in 0..iterations {
        reset(windows);
        extend(black_box(windows));
        total = total.wrapping_add(black_box(checksum(windows)));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        reset(windows);
        extend(black_box(windows));
        black_box(checksum(windows));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocations = ALLOC.snapshot().delta(before);
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_compile: seconds * 1e9 / iterations as f64,
        cycles_per_compile: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        throughput: iterations as f64 / seconds,
        allocations,
        checksum: total,
    }
}

fn run(title: &str, count: usize, iterations: usize) {
    let mut old_windows = fixture(count);
    let mut new_windows = old_windows.clone();
    let old = measure(iterations, &mut old_windows, extend_reference);
    let new = measure(iterations, &mut new_windows, extend_runtime_mod_sustains);
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);
    println!("\n{title}");
    print_result("old", iterations, &old);
    print_result("new", iterations, &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% churn",
        percent_change(old.ns_per_compile, new.ns_per_compile),
        percent_change(
            old.cycles_per_compile.unwrap_or(f64::NAN),
            new.cycles_per_compile.unwrap_or(f64::NAN),
        ),
        percent_change(old.throughput, new.throughput),
        percent_change(
            old.allocations.churn_bytes as f64,
            new.allocations.churn_bytes as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let compiles = iterations as f64;
    println!(
        "  {label:<3} {:>11.2} ns/compile  {:>11.2} cycles/compile  {:>8.1} compile/s  \
         {:>5.2} alloc/compile  {:>5.2} realloc/compile  {:>5.2} free/compile  \
         {:>8.1} churn B/compile",
        result.ns_per_compile,
        result.cycles_per_compile.unwrap_or(f64::NAN),
        result.throughput,
        result.allocations.allocs as f64 / compiles,
        result.allocations.reallocs as f64 / compiles,
        result.allocations.frees as f64 / compiles,
        result.allocations.churn_bytes as f64 / compiles,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocations.allocs, 0);
    assert_eq!(result.allocations.reallocs, 0);
    assert_eq!(result.allocations.frees, 0);
    assert_eq!(result.allocations.churn_bytes, 0);
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
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
    run("typical mod-chart sustain setup (192 windows)", 192, 5_000);
    run("dense mod-chart sustain setup (1024 windows)", 1024, 300);
}
