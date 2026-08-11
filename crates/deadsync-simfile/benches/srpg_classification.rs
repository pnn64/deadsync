use deadsync_simfile::event_intro::is_srpg_event_group;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ITERATIONS: usize = 2_000_000;
const GROUPS: [&str; 8] = [
    "Stamina RPG 10",
    "STAMINA rPg 9 Unlocks - Player",
    "SRPG10",
    "ITL Online 2026",
    "Stamina RPG Songs",
    "Regular Pack 12",
    "prefix-sRpG-2026-suffix",
    "  RPG Songs  ",
];

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

// SAFETY: allocator operations delegate unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
        // SAFETY: the pointer-layout pair came from the delegated allocator.
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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            dealloc_bytes: self.dealloc_bytes - before.dealloc_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.dealloc_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn legacy(group: &str) -> bool {
    let lower = group.trim().to_ascii_lowercase();
    lower.chars().any(|c| c.is_ascii_digit())
        && (lower.contains("stamina rpg") || lower.contains("srpg"))
}

fn measure(mut classify: impl FnMut(&str) -> bool) -> BenchResult {
    for index in 0..ITERATIONS / 20 {
        black_box(classify(GROUPS[index % GROUPS.len()]));
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for index in 0..ITERATIONS {
        checksum =
            checksum.rotate_left(1) ^ u64::from(classify(black_box(GROUPS[index % GROUPS.len()])));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for index in 0..ITERATIONS {
        black_box(classify(black_box(GROUPS[index % GROUPS.len()])));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    BenchResult {
        ns_per_op: elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ITERATIONS as f64),
        allocated,
        checksum,
    }
}

fn main() {
    for group in GROUPS {
        assert_eq!(legacy(group), is_srpg_event_group(group));
    }
    let old = measure(legacy);
    let new = measure(is_srpg_event_group);
    assert_eq!(old.checksum, new.checksum, "classification diverged");

    println!("SRPG pack classification");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(1.0 / old.ns_per_op, 1.0 / new.ns_per_op),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let ops = ITERATIONS as f64;
    println!(
        "  {label:<3} {:>9.2} ns/op  {:>9.2} cycles/op  {:>8.3} Mop/s  \
         {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>8.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.deallocs as f64 / ops,
        result.allocated.churn_bytes() as f64 / ops,
    );
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
