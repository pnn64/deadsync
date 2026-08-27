use deadsync_heart_rate::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ID_EVENTS: usize = 200_000;
const SELECTION_TICKS: usize = 200_000;
const DISCOVERY_TICKS: usize = 30_000;
const SAMPLES: usize = 32;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    dealloc_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            dealloc_bytes: self.dealloc_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocator operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.dealloc_bytes
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
    deallocs: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    dealloc_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            dealloc_bytes: self.dealloc_bytes - before.dealloc_bytes,
        }
    }

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.deallocs
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.dealloc_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    ops_per_second: f64,
    worst_ns_per_op: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(operations: usize, operation: fn(usize) -> u64) -> BenchResult {
    let sample_operations = (operations / 20).max(1);
    black_box(operation(sample_operations));

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation(operations));
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation(operations));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    let mut worst_ns_per_op = 0.0f64;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        black_box(operation(sample_operations));
        worst_ns_per_op = worst_ns_per_op
            .max(started.elapsed().as_secs_f64() * 1_000_000_000.0 / sample_operations as f64);
    }

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1_000_000_000.0 / operations as f64,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64),
        ops_per_second: operations as f64 / seconds,
        worst_ns_per_op,
        allocated,
        checksum,
    }
}

fn print_pair(name: &str, operations: usize, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", operations, old);
    print_result("new", operations, new);
    assert_eq!(new.checksum, old.checksum, "{name} output diverged");
    assert_eq!(new.allocated.operations(), 0, "{name} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{name} new path churned");
}

fn print_result(label: &str, operations: usize, result: &BenchResult) {
    let count = operations as f64;
    println!(
        "{label:<4} {:>9.2} ns/op  {:>9.2} cycles/op  {:>8.3} Mop/s  \
         worst {:>9.2} ns  {:>5.2} alloc/op  {:>5.2} realloc/op  \
         {:>5.2} free/op  {:>9.1} churn B/op  {:016x}",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.ops_per_second / 1_000_000.0,
        result.worst_ns_per_op,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.deallocs as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
        result.checksum,
    );
}

fn main() {
    let ids_old = measure(ID_EVENTS, bench_support::stable_ids_old);
    let ids_new = measure(ID_EVENTS, bench_support::stable_ids_new);
    assert_eq!(ids_old.allocated.allocs, (ID_EVENTS * 3) as u64);
    assert_eq!(ids_old.allocated.deallocs, (ID_EVENTS * 3) as u64);
    print_pair(
        "shared desired and monitor-event device IDs",
        ID_EVENTS,
        &ids_old,
        &ids_new,
    );

    let selection_old = measure(SELECTION_TICKS, bench_support::fixed_selection_old);
    let selection_new = measure(SELECTION_TICKS, bench_support::fixed_selection_new);
    assert!(selection_old.allocated.allocs >= SELECTION_TICKS as u64 * 3);
    print_pair(
        "fixed two-player selection checks",
        SELECTION_TICKS,
        &selection_old,
        &selection_new,
    );

    let discovery_old = measure(DISCOVERY_TICKS, bench_support::unchanged_discovery_old);
    let discovery_new = measure(DISCOVERY_TICKS, bench_support::unchanged_discovery_new);
    assert!(discovery_old.allocated.allocs > DISCOVERY_TICKS as u64);
    print_pair(
        "unchanged 16-device discovery publication",
        DISCOVERY_TICKS,
        &discovery_old,
        &discovery_new,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
