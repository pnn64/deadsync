use deadsync_audio_replaygain::{Priority, bench_support};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

type Workload =
    fn(bench_support::PrewarmBatchFixture, Priority) -> bench_support::PrewarmBatchOutcome;

const PATHS: usize = 512;
const OPERATIONS: usize = 160;
const SAMPLES: usize = 17;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

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

// SAFETY: all requests are forwarded unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while measurement is enabled.
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

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns_per_path: f64,
    p95_ns_per_path: f64,
    median_cycles_per_path: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(paths: &[PathBuf], warm_stride: usize, workload: Workload) -> Row {
    for _ in 0..3 {
        black_box(workload(
            bench_support::prewarm_batch_fixture(paths, warm_stride),
            Priority::Background,
        ));
    }

    let measured_paths = (OPERATIONS * PATHS) as f64;
    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let fixtures = (0..OPERATIONS)
            .map(|_| bench_support::prewarm_batch_fixture(paths, warm_stride))
            .collect::<Vec<_>>();
        let mut fixtures = fixtures.into_iter();
        let mut outcomes = Vec::with_capacity(OPERATIONS);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for fixture in fixtures.by_ref() {
            outcomes.push(black_box(workload(fixture, Priority::Background)));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / measured_paths);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_paths)
        {
            cycles.push(elapsed);
        }
        for outcome in &outcomes {
            checksum = checksum.wrapping_add(outcome.checksum());
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let fixture = bench_support::prewarm_batch_fixture(paths, warm_stride);
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let outcome = black_box(workload(fixture, Priority::Background));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(&outcome);

    Row {
        median_ns_per_path: times[SAMPLES / 2],
        p95_ns_per_path: times[SAMPLES * 95 / 100],
        median_cycles_per_path: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, old: &Row, new: &Row, expect_less_churn: bool) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    if expect_less_churn {
        assert!(
            new.alloc.reallocs < old.alloc.reallocs,
            "{title} did not reduce queue reallocations"
        );
        assert!(
            new.alloc.churn() < old.alloc.churn(),
            "{title} did not reduce allocation churn"
        );
    }

    println!("\n{title}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% frees  \
         {:>7.2}% churn",
        change(old.median_ns_per_path, new.median_ns_per_path),
        change(
            old.median_cycles_per_path.unwrap_or(f64::NAN),
            new.median_cycles_per_path.unwrap_or(f64::NAN),
        ),
        change(throughput(old), throughput(new)),
        change(old.p95_ns_per_path, new.p95_ns_per_path),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.frees as f64, new.alloc.frees as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>10.2} ns/path  {:>10.2} cycles/path  {:>10.2} p95 ns  \
         {:>8.2} Mpath/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns_per_path,
        row.median_cycles_per_path.unwrap_or(f64::NAN),
        row.p95_ns_per_path,
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row) -> f64 {
    1e9 / row.median_ns_per_path
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let paths = (0..PATHS)
        .map(|index| PathBuf::from(format!("Songs/Pack/Song {index:04}/music.ogg")))
        .collect::<Vec<_>>();

    for (title, warm_stride, expect_less_churn) in [
        ("cold unique pack", 0, true),
        ("duplicate-heavy mixed pack", 3, true),
        ("fully cached pack revisit", 1, false),
    ] {
        let old = measure(
            black_box(&paths),
            warm_stride,
            bench_support::prewarm_batch_reference,
        );
        let new = measure(black_box(&paths), warm_stride, bench_support::prewarm_batch);
        print_pair(title, &old, &new, expect_less_churn);
    }
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
