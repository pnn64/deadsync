use deadsync_audio_analysis::cache::bench_support;
use deadsync_audio_analysis::{
    ReplayGainCacheEntry, ReplayGainCacheFile, ReplayGainInfo, bench_support as gain_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const ENTRIES: usize = 8_192;
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
    for _ in 0..3 {
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

fn print_pair(title: &str, units: usize, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title} ({units} units/op)");
    print_row("old", units, old);
    print_row("new", units, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old, units), throughput(new, units)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, units: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns/op  {:>11.0} cycles/op  {:>11.0} p95 ns  \
         {:>8.2} Munit/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row, units) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row, units: usize) -> f64 {
    units as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let infos = gain_infos();
    let max_error = gain_bench::max_gain_error(&infos);
    assert!(max_error <= 2e-6, "gain error {max_error}");
    println!("maximum gain error versus old formula: {max_error:.9}");
    print_pair(
        "ReplayGain linear-gain conversion",
        infos.len(),
        &measure(256, || gain_bench::gains_old(black_box(&infos))),
        &measure(256, || gain_bench::gains_new(black_box(&infos))),
    );

    let entries = entries();
    print_pair(
        "ReplayGain cache ordering",
        entries.len(),
        &measure(64, || bench_support::sort_entries_old(black_box(&entries))),
        &measure(64, || bench_support::sort_entries_new(black_box(&entries))),
    );

    let payload = ReplayGainCacheFile {
        entries: entries.clone(),
    };
    print_pair(
        "ReplayGain cache encoding",
        payload.entries.len(),
        &measure(128, || bench_support::encode_cache_old(black_box(&payload))),
        &measure(128, || bench_support::encode_cache_new(black_box(&payload))),
    );
}

fn entries() -> Vec<ReplayGainCacheEntry> {
    let mut seed = 0x0123_4567_89ab_cdef_u64;
    (0..ENTRIES)
        .map(|index| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ReplayGainCacheEntry {
                path_hash: seed ^ index as u64,
                mtime_unix_nanos: 1_700_000_000_000_000_000_u64 + index as u64,
                content_hash: seed.rotate_left(23),
                lufs: -30.0 + (index % 240) as f32 * 0.1,
                true_peak_linear: 0.25 + (index % 70) as f32 * 0.01,
            }
        })
        .collect()
}

fn gain_infos() -> Vec<ReplayGainInfo> {
    (0..ENTRIES)
        .map(|index| ReplayGainInfo {
            lufs: -69.4 + (index % 795) as f32 * 0.1,
            true_peak_linear: (index % 201) as f32 * 0.01,
        })
        .collect()
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
