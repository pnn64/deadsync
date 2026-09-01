use deadsync_net::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const SAMPLES: usize = 21;

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

// SAFETY: requests are delegated unchanged to `System`; relaxed counters only
// observe this single-threaded benchmark while measurement is enabled.
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
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(operations: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..operations.min(4_096) {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..operations {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / operations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn checksum(text: &str) -> u64 {
    text.len() as u64
        + u64::from(text.as_bytes().first().copied().unwrap_or_default())
        + u64::from(text.as_bytes().last().copied().unwrap_or_default())
}

fn assert_faster(title: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title} cycle count regressed");
    }
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "{title} allocation churn did not improve"
    );
}

fn print_pair(title: &str, bytes: usize, old: &Row, new: &Row) {
    println!("\n{title} ({bytes} bytes/op)");
    print_row("old", bytes, old);
    print_row("new", bytes, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old, bytes), throughput(new, bytes)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
    assert_faster(title, old, new);
}

fn print_row(label: &str, bytes: usize, row: &Row) {
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} p95 ns  \
         {:>9.2} MiB/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row, bytes) / (1024.0 * 1024.0),
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row, bytes: usize) -> f64 {
    bytes as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let timeout = "ArrowCloud request failed while waiting for the remote score service: TiMeD OuT";
    let old = measure(500_000, || {
        u64::from(bench_support::timeout_old(black_box(timeout)))
    });
    let new = measure(500_000, || {
        u64::from(bench_support::timeout_new(black_box(timeout)))
    });
    assert_eq!(new.alloc.allocs, 0, "timeout classifier allocated");
    print_pair(
        "case-insensitive timeout classification",
        timeout.len(),
        &old,
        &new,
    );

    let body = br#"{"status":"ok","service":"ArrowCloud","scores":[{"chart":"ABC123","wife":0.9987}],"message":"bounded response body"}"#;
    let max_bytes = 2 * 1024 * 1024;
    let old = measure(4_096, || {
        let decoded = bench_support::known_length_body_old(black_box(body), max_bytes);
        checksum(&decoded)
    });
    let new = measure(4_096, || {
        let decoded = bench_support::known_length_body_new(black_box(body), max_bytes);
        checksum(&decoded)
    });
    assert_eq!(new.alloc.reallocs, 0, "known-length body reallocated");
    print_pair("known-length bounded body read", body.len(), &old, &new);

    let response = format!(
        "score submission rejected: {}",
        "Unicode café response detail; ".repeat(32)
    );
    let old = measure(32_768, || {
        let owned = black_box(response.as_str()).to_owned();
        let snippet = bench_support::snippet_old(&owned);
        checksum(&snippet)
    });
    let new = measure(32_768, || {
        let owned = black_box(response.as_str()).to_owned();
        let snippet = bench_support::snippet_new(owned);
        checksum(&snippet)
    });
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "owned snippet did not eliminate its copy"
    );
    print_pair("owned response log snippet", response.len(), &old, &new);
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
