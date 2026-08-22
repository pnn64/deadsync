use deadsync_notefield::performance::{HoldMeshPoolBench, ReferenceHoldMeshPoolBench};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 100;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while the benchmark gate is enabled.
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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_frame: f64,
    p95_ns: f64,
    cycles_per_frame: Option<f64>,
    pairs_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, pairs: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(1) {
        black_box(frame());
    }

    let batch = (iterations / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(iterations.div_ceil(batch));
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations.div_ceil(batch) {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(frame()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0_u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_iterations = iterations.div_ceil(batch) * batch;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_frame: seconds * 1e9 / measured_iterations as f64,
        p95_ns: sample_ns[sample_ns.len() * 95 / 100],
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_iterations as f64),
        pairs_per_second: measured_iterations as f64 * pairs as f64 / seconds,
        allocated,
        checksum,
    }
}

fn run(
    title: &str,
    iterations: usize,
    max_pairs: usize,
    pairs: usize,
    diffuse_vertices: usize,
    glow_vertices: usize,
    retained_frames: usize,
) {
    let capacity = diffuse_vertices.max(glow_vertices);
    let mut old_pool = ReferenceHoldMeshPoolBench::new(max_pairs, capacity, retained_frames);
    let mut new_pool = HoldMeshPoolBench::new(max_pairs, capacity, retained_frames);
    for _ in 0..retained_frames + 2 {
        assert_eq!(
            old_pool.frame(pairs, diffuse_vertices, glow_vertices),
            new_pool.frame(pairs, diffuse_vertices, glow_vertices)
        );
    }

    let old = measure(iterations, pairs, || {
        old_pool.frame(pairs, diffuse_vertices, glow_vertices)
    });
    let new = measure(iterations, pairs, || {
        new_pool.frame(pairs, diffuse_vertices, glow_vertices)
    });
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\n{title}");
    print_result("old", iterations, &old);
    print_result("new", iterations, &new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_frame, new.ns_per_frame),
        percent_change(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_change(old.pairs_per_second, new.pairs_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>10.2} p95 ns  \
         {:>8.2} Mpair/s  {:>5.2} alloc/frame  {:>5.2} realloc/frame  \
         {:>5.2} free/frame  {:>10.1} churn B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.p95_ns,
        result.pairs_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.frees, 0);
    assert_eq!(result.allocated.churn_bytes(), 0);
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
    run("one diffuse hold body", 2_000_000, 20, 1, 384, 0, 2);
    run("four diffuse hold caps", 1_000_000, 40, 4, 6, 0, 2);
    run("eight glowing bodies", 500_000, 20, 8, 384, 384, 1);
}
