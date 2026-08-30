use deadsync_simfile::bpm::{
    BpmTimeline, BpmTimelineReference, beat_at_sec_from_bpms, beat_at_sec_from_bpms_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 25;
const PARSE_BATCH: usize = 512;
const FRAME_QUERIES: usize = 4_096;
const LONG_QUERIES: usize = 8_192;

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

// SAFETY: allocation requests are delegated unchanged to `System`; relaxed
// counters only observe this single-threaded benchmark while its gate is set.
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
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> Row {
    for _ in 0..5 {
        black_box(operation());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(operation());
        times.push(started.elapsed().as_secs_f64() * 1e9);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn checksum_push(checksum: u64, value: f64) -> u64 {
    checksum.rotate_left(9).wrapping_mul(1_099_511_628_211) ^ value.to_bits()
}

fn run_queries(count: usize, mut query: impl FnMut(usize) -> f64) -> u64 {
    let mut checksum = 0u64;
    for index in 0..count {
        checksum = checksum_push(checksum, black_box(query(index)));
    }
    checksum
}

fn print_pair(title: &str, items: usize, old: &Row, new: &Row, require_less_churn: bool) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{title} latency regressed: old={}ns new={}ns",
        old.median_ns,
        new.median_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} cycles regressed: old={old_cycles} new={new_cycles}"
        );
    }
    assert!(
        new.alloc.allocs <= old.alloc.allocs,
        "{title} allocs regressed"
    );
    assert!(
        new.alloc.reallocs <= old.alloc.reallocs,
        "{title} reallocs regressed"
    );
    assert!(
        new.alloc.frees <= old.alloc.frees,
        "{title} frees regressed"
    );
    if require_less_churn {
        assert!(
            new.alloc.churn() < old.alloc.churn(),
            "{title} churn did not improve"
        );
    } else {
        assert!(
            new.alloc.churn() <= old.alloc.churn(),
            "{title} churn regressed"
        );
    }

    println!("\n{title} ({items} lookups)");
    print_row("old", items, old);
    print_row("new", items, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(items, old), throughput(items, new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, items: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns  {:>11.0} cycles  {:>11.0} p95 ns  \
         {:>8.3} Mlookup/s  {:>6} allocs  {:>5} reallocs  {:>6} frees  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(items, row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(items: usize, row: &Row) -> f64 {
    items as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let common = "0=128,16=160,32=96,48=192,64=144,80=180,96=120,112=200";
    let old = measure(|| {
        run_queries(PARSE_BATCH, |index| {
            beat_at_sec_from_bpms_reference(common, 12.0 + (index % 64) as f64 * 0.125)
        })
    });
    let new = measure(|| {
        run_queries(PARSE_BATCH, |index| {
            beat_at_sec_from_bpms(common, 12.0 + (index % 64) as f64 * 0.125)
        })
    });
    print_pair(
        "inline BPM timeline construction",
        PARSE_BATCH,
        &old,
        &new,
        true,
    );

    let cached = BpmTimeline::new(common);
    let old = measure(|| {
        run_queries(FRAME_QUERIES, |index| {
            beat_at_sec_from_bpms_reference(common, 8.0 + (index % 512) as f64 * 0.03125)
        })
    });
    let new = measure(|| {
        run_queries(FRAME_QUERIES, |index| {
            cached.beat_at_sec(8.0 + (index % 512) as f64 * 0.03125)
        })
    });
    print_pair(
        "cached per-frame preview beat",
        FRAME_QUERIES,
        &old,
        &new,
        true,
    );

    let long_map = long_map();
    let reference = BpmTimelineReference::new(&long_map);
    let timeline = BpmTimeline::new(&long_map);
    let old = measure(|| {
        run_queries(LONG_QUERIES, |index| {
            reference.beat_at_sec(250.0 + (index % 1_024) as f64 * 0.0625)
        })
    });
    let new = measure(|| {
        run_queries(LONG_QUERIES, |index| {
            timeline.beat_at_sec(250.0 + (index % 1_024) as f64 * 0.0625)
        })
    });
    print_pair(
        "cumulative binary BPM lookup",
        LONG_QUERIES,
        &old,
        &new,
        false,
    );
}

fn long_map() -> String {
    let mut map = String::with_capacity(4_096);
    for index in 0..256 {
        if index != 0 {
            map.push(',');
        }
        write!(&mut map, "{}={}", index * 4, 90 + index % 151).unwrap();
    }
    map
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
