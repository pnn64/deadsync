use deadsync_score::{
    PlayedChartHistory, benchmark_history_from_names, benchmark_play_counts_from_names,
    benchmark_recent_from_names, parse_score_file_name,
};
use rustc_hash::FxBuildHasher;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FILES: usize = 8_192;
const CHARTS: usize = 512;
const ITERATIONS: usize = 160;
const SAMPLES: usize = 40;

type FxMap<K, V> = HashMap<K, V, FxBuildHasher>;

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
// only observe successful calls while the single-threaded benchmark gate is on.
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

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    files_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..8 {
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

    let allocation_runs = ITERATIONS / 4;
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
        files_per_second: measured_runs as f64 * FILES as f64 / seconds,
        allocated,
        allocation_runs,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
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
        percent_change(old.files_per_second, new.files_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = result.allocation_runs as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>11.2} cycles/op  {:>11.2} p95 ns  \
         {:>7.2} Mfile/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
         {:>8.1} free/op  {:>11.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.files_per_second / 1e6,
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

fn fixture_names() -> Vec<String> {
    (0..FILES)
        .map(|play| {
            let chart = play * 73 % CHARTS;
            format!(
                "{chart:040x}-{}.bin",
                1_700_000_000_000i64 + play as i64 * 997
            )
        })
        .collect()
}

fn rank_counts<S: BuildHasher>(counts: HashMap<String, u32, S>) -> Vec<(String, u32)> {
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

fn entry_fx_counts(names: &[String]) -> Vec<(String, u32)> {
    let mut counts = FxMap::default();
    for name in names {
        let Some((chart_hash, _)) = parse_score_file_name(name) else {
            continue;
        };
        counts
            .entry(chart_hash.to_owned())
            .and_modify(|count: &mut u32| *count = count.saturating_add(1))
            .or_insert(1);
    }
    rank_counts(counts)
}

fn get_mut_std_counts(names: &[String]) -> Vec<(String, u32)> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for name in names {
        let Some((chart_hash, _)) = parse_score_file_name(name) else {
            continue;
        };
        match counts.get_mut(chart_hash) {
            Some(count) => *count = count.saturating_add(1),
            None => {
                counts.insert(chart_hash.to_owned(), 1);
            }
        }
    }
    rank_counts(counts)
}

fn separate_history(names: &[String]) -> PlayedChartHistory {
    PlayedChartHistory {
        recent_chart_hashes: benchmark_recent_from_names(names),
        played_chart_counts: benchmark_play_counts_from_names(names),
    }
}

fn checksum_strings(values: &[String]) -> u64 {
    values.iter().fold(0xcbf2_9ce4_8422_2325, |hash, value| {
        value.bytes().fold(hash, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        })
    })
}

fn checksum_counts(values: &[(String, u32)]) -> u64 {
    values
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, (value, count)| {
            value.bytes().fold(hash ^ u64::from(*count), |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
            })
        })
}

fn checksum_history(history: &PlayedChartHistory) -> u64 {
    checksum_strings(&history.recent_chart_hashes)
        .rotate_left(17)
        .wrapping_add(checksum_counts(&history.played_chart_counts))
}

fn main() {
    let names = fixture_names();

    let old = measure(|| checksum_counts(black_box(&entry_fx_counts(black_box(&names)))));
    let new = measure(|| {
        checksum_counts(black_box(&benchmark_play_counts_from_names(black_box(
            &names,
        ))))
    });
    print_pair("allocate owned key on miss only", &old, &new);
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.churn_bytes() < old.allocated.churn_bytes());

    let old = measure(|| checksum_counts(black_box(&get_mut_std_counts(black_box(&names)))));
    let new = measure(|| {
        checksum_counts(black_box(&benchmark_play_counts_from_names(black_box(
            &names,
        ))))
    });
    print_pair("FxHash for trusted chart hashes", &old, &new);

    let old = measure(|| checksum_history(black_box(&separate_history(black_box(&names)))));
    let new =
        measure(|| checksum_history(black_box(&benchmark_history_from_names(black_box(&names)))));
    print_pair("one history pass for recent plus counts", &old, &new);
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.churn_bytes() < old.allocated.churn_bytes());
}
