use deadsync_import::ini::{SimpleIni, bench_support};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SECTIONS: usize = 128;
const KEYS_PER_SECTION: usize = 64;
const ITEMS: usize = SECTIONS * KEYS_PER_SECTION;
const ITERATIONS: usize = 60;
const SAMPLES: usize = 20;

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

// SAFETY: requests are delegated unchanged to `System`; relaxed counters only
// observe successful calls while this single-threaded benchmark enables them.
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

struct BenchResult {
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(operation());
    }

    let batch = (ITERATIONS / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(SAMPLES);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let allocation_runs = ITERATIONS / 15;
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..allocation_runs {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_runs = batch * SAMPLES;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_runs as f64,
        p95_ns: sample_ns[(sample_ns.len() * 95 / 100).min(sample_ns.len() - 1)],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_runs as f64),
        items_per_second: measured_runs as f64 * ITEMS as f64 / seconds,
        allocated,
        allocation_runs,
        checksum,
    }
}

#[derive(Clone, Copy)]
enum AllocationGuard {
    OperationsDrop,
    Stable,
    Zero,
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult, guard: AllocationGuard) {
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(new.ns_per_op < old.ns_per_op, "{title} latency regressed");
    assert!(
        new.items_per_second > old.items_per_second,
        "{title} throughput regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.cycles_per_op, new.cycles_per_op) {
        assert!(new_cycles < old_cycles, "{title} CPU cycles regressed");
    }
    match guard {
        AllocationGuard::OperationsDrop => {
            assert!(
                new.allocated.operations() < old.allocated.operations(),
                "{title} allocation operations did not improve"
            );
            assert!(
                new.allocated.churn_bytes() < old.allocated.churn_bytes(),
                "{title} memory churn did not improve"
            );
        }
        AllocationGuard::Stable => {
            assert_eq!(new.allocated.allocs, old.allocated.allocs);
            assert_eq!(new.allocated.reallocs, old.allocated.reallocs);
            assert_eq!(new.allocated.frees, old.allocated.frees);
            assert!(
                new.allocated.churn_bytes() <= old.allocated.churn_bytes(),
                "{title} memory churn regressed"
            );
        }
        AllocationGuard::Zero => {
            assert!(old.allocated.operations() > 0);
            assert_eq!(new.allocated.operations(), 0);
            assert_eq!(new.allocated.churn_bytes(), 0);
        }
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = result.allocation_runs as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>11.2} cycles/op  {:>11.2} p95 ns  \
         {:>7.2} Mitem/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
         {:>8.1} free/op  {:>11.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.items_per_second / 1e6,
        result.allocated.allocs as f64 / runs,
        result.allocated.reallocs as f64 / runs,
        result.allocated.frees as f64 / runs,
        result.allocated.churn_bytes() as f64 / runs,
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

fn fixture() -> (String, Vec<(String, String)>) {
    let mut content = String::with_capacity(ITEMS * 48);
    let mut queries = Vec::with_capacity(ITEMS);
    for section in 0..SECTIONS {
        writeln!(content, "[Section {section:03}]").expect("write section");
        for key in 0..KEYS_PER_SECTION {
            writeln!(
                content,
                "Key {key:03} = benchmark-value-{section:03}-{key:03}"
            )
            .expect("write key");
            queries.push((format!("Section {section:03}"), format!("Key {key:03}")));
        }
    }
    (content, queries)
}

fn main() {
    let (content, queries) = fixture();
    let ini: SimpleIni = bench_support::load(&content);

    print_pair(
        "borrow current INI section",
        &measure(|| bench_support::parse_cloned_sections(black_box(&content))),
        &measure(|| bench_support::parse_borrowed_sections(black_box(&content))),
        AllocationGuard::OperationsDrop,
    );
    print_pair(
        "fast hashing for INI keys",
        &measure(|| bench_support::parse_borrowed_sections(black_box(&content))),
        &measure(|| bench_support::parse_fast_hash(black_box(&content))),
        AllocationGuard::Stable,
    );
    print_pair(
        "borrow INI lookup values",
        &measure(|| bench_support::lookup_cloned(black_box(&ini), black_box(&queries))),
        &measure(|| bench_support::lookup_borrowed(black_box(&ini), black_box(&queries))),
        AllocationGuard::Zero,
    );
}
