use deadsync_input_native::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;
const REGISTRY_OPS: usize = 4_000;
const PARSE_OPS: usize = 2_000;
const SERIALIZE_OPS: usize = 2_000;
const UUIDS_PER_OP: usize = 64;

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

// SAFETY: every request is delegated unchanged to `System`; relaxed counters
// observe only this single-threaded benchmark while the gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` was supplied by the allocator caller.
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(operations: usize, mut op: impl FnMut(usize) -> u64) -> Row {
    for seed in 0..4 {
        black_box(op(seed));
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for sample in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut sample_checksum = 0_u64;
        for operation in 0..operations {
            sample_checksum = sample_checksum.wrapping_add(black_box(op(
                operation.wrapping_add(sample.wrapping_mul(operations))
            )));
        }
        let cycle_end = cycle_counter();
        times.push(started.elapsed().as_secs_f64() * 1e9 / operations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64)
        {
            cycles.push(elapsed);
        }
        checksum = checksum.wrapping_add(sample_checksum);
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut alloc_checksum = 0_u64;
    for operation in 0..operations {
        alloc_checksum = alloc_checksum.wrapping_add(black_box(op(operation)));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(alloc_checksum);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, operations: usize, items: usize, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_row("old", operations, items, old);
    print_row("new", operations, items, new);
    println!(
        "  change: {:+.2}% median  {:+.2}% cycles  {:+.2}% throughput  \
         {:+.2}% p95  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old, items), throughput(new, items)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(
            old.alloc.churn_bytes() as f64,
            new.alloc.churn_bytes() as f64
        ),
    );
}

fn print_row(label: &str, operations: usize, items: usize, row: &Row) {
    let count = operations as f64;
    println!(
        "  {label:<3} {:>10.1} ns/op  p95 {:>10.1} ns  {:>10.1} cycles/op  \
         {:>8.2} Mitem/s  {:>6.2} alloc/op  {:>6.2} realloc/op  \
         {:>6.2} free/op  {:>10.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput(row, items) / 1e6,
        row.alloc.allocs as f64 / count,
        row.alloc.reallocs as f64 / count,
        row.alloc.frees as f64 / count,
        row.alloc.churn_bytes() as f64 / count,
    );
}

fn throughput(row: &Row, items: usize) -> f64 {
    items as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
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
    let old = measure(REGISTRY_OPS, bench_support::assignment_old);
    let new = measure(REGISTRY_OPS, bench_support::assignment_new);
    assert!(old.alloc.operations() > 0);
    assert_eq!(new.alloc.operations(), 0);
    print_pair(
        "64-device stable-index registry",
        REGISTRY_OPS,
        UUIDS_PER_OP,
        &old,
        &new,
    );

    let raw = bench_support::serialized_fixture();
    let old = measure(PARSE_OPS, |_| bench_support::parse_old(black_box(&raw)));
    let new = measure(PARSE_OPS, |_| bench_support::parse_new(black_box(&raw)));
    assert!(old.alloc.operations() > 0);
    assert_eq!(new.alloc.operations(), 0);
    print_pair(
        "bounded serialized-order load",
        PARSE_OPS,
        UUIDS_PER_OP + 2,
        &old,
        &new,
    );

    let old = measure(SERIALIZE_OPS, bench_support::serialize_old);
    let new = measure(SERIALIZE_OPS, bench_support::serialize_new);
    assert!(old.alloc.allocs > new.alloc.allocs);
    assert!(old.alloc.churn_bytes() > new.alloc.churn_bytes());
    print_pair(
        "64-device order serialization",
        SERIALIZE_OPS,
        UUIDS_PER_OP,
        &old,
        &new,
    );
}
