use deadsync_online::srpg_shop::{
    SrpgShopItem, parse_catalog_for_bench, parse_catalog_reference_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 2_048;
const SAMPLES: usize = 21;
const WARMUPS: usize = 3;
const OPS: usize = 4;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all operations delegate unchanged to `System`. The relaxed counters
// are enabled only around one single-threaded parse operation.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
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
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..WARMUPS {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample % 2 == 0 {
            (timed_sample(&mut old_op), timed_sample(&mut new_op))
        } else {
            let new_sample = timed_sample(&mut new_op);
            let old_sample = timed_sample(&mut old_op);
            (old_sample, new_sample)
        };
        old_times.push(old_sample.0);
        new_times.push(new_sample.0);
        if let Some(cycles) = old_sample.1 {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_sample.1 {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sample.2;
        new_checksum ^= new_sample.2;
    }

    let old_allocated = measured_allocations(&mut old_op);
    let new_allocated = measured_allocations(&mut new_op);
    (
        bench_result(old_times, old_cycles, old_allocated, old_checksum),
        bench_result(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn timed_sample(op: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..OPS {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9 / OPS as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS as f64);
    (elapsed, cycles, checksum)
}

fn measured_allocations(op: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn bench_result(
    mut times: Vec<f64>,
    mut cycles: Vec<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
) -> BenchResult {
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated,
        checksum,
    }
}

fn fixture() -> String {
    let mut body = String::with_capacity(ROWS * 210);
    body.push_str("{\"data\":[");
    for index in 0..ROWS {
        if index != 0 {
            body.push(',');
        }
        let shuffled = index.wrapping_mul(811) % ROWS;
        let kind = usize::from(!shuffled.is_multiple_of(5));
        let group = shuffled.wrapping_mul(37) % 19;
        let difficulty = 8 + shuffled.wrapping_mul(17) % 30;
        let bpm = 90 + shuffled.wrapping_mul(29) % 330;
        let cost = 1_000 + shuffled.wrapping_mul(9_973) % 900_000;
        let cost_high = cost / 1_000;
        let cost_low = cost % 1_000;
        write!(
            body,
            "[\"{}\",\"chart.png\",\"<b>Tournament &amp; Pack {shuffled:05}</b>\",\"  Purchase   row {shuffled:05} now  \",\"Difficulty: {difficulty}|Speed Tier: {bpm} BPM\",\"2\",\"0\",\"{cost_high},{cost_low:03}\",\"{group}\",\"0\",\"0\",\"{kind}\",\"{difficulty}\",\"{bpm}\",\"0\"]",
            100_000 + shuffled,
        )
        .unwrap();
    }
    body.push_str("]}");
    body
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

fn item_checksum(item: &SrpgShopItem) -> u64 {
    let kind = match item.kind {
        deadsync_online::srpg_shop::SrpgShopItemKind::Song => 1u64,
        deadsync_online::srpg_shop::SrpgShopItemKind::Relic => 2,
    };
    text_checksum(&item.item_id)
        .wrapping_mul(257)
        .wrapping_add(text_checksum(&item.name))
        .wrapping_mul(257)
        .wrapping_add(text_checksum(&item.description))
        .wrapping_mul(257)
        .wrapping_add(text_checksum(&item.effect))
        ^ item.cost.unwrap_or(u64::MAX).rotate_left(7)
        ^ u64::from(item.difficulty.unwrap_or(u32::MAX)).rotate_left(17)
        ^ u64::from(item.bpm.unwrap_or(u32::MAX)).rotate_left(29)
        ^ kind.rotate_left(41)
}

fn catalog_checksum(items: &[SrpgShopItem]) -> u64 {
    items.iter().fold(items.len() as u64, |sum, item| {
        sum.wrapping_mul(65_537).wrapping_add(item_checksum(item))
    })
}

fn main() {
    let body = fixture();
    let expected = parse_catalog_reference_for_bench(&body, 0, 0).unwrap();
    let actual = parse_catalog_for_bench(&body, 0, 0).unwrap();
    assert_eq!(actual, expected, "catalog behavior diverged");

    let (old, new) = measure_pair(
        || catalog_checksum(&parse_catalog_reference_for_bench(black_box(&body), 0, 0).unwrap()),
        || catalog_checksum(&parse_catalog_for_bench(black_box(&body), 0, 0).unwrap()),
    );
    assert_eq!(old.checksum, new.checksum, "catalog checksum diverged");

    println!("SRPG catalog parse and sort ({ROWS} rows)");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% p95  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        improvement(old.median_ns, new.median_ns),
        improvement(old.p95_ns, new.p95_ns),
        improvement(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(throughput(&old), throughput(&new)),
        improvement(old.allocated.allocs as f64, new.allocated.allocs as f64),
        improvement(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        improvement(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );

    assert!(new.median_ns < old.median_ns, "median regressed");
    assert!(new.p95_ns < old.p95_ns, "p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "cycles regressed");
    }
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.reallocs < old.allocated.reallocs);
    assert!(new.allocated.frees < old.allocated.frees);
    assert!(new.allocated.allocated_bytes < old.allocated.allocated_bytes);
    assert!(new.allocated.churn() < old.allocated.churn());
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>10.0} rows/s  \
         {:>8} alloc  {:>5} realloc  {:>8} free  {:>12} B alloc  {:>12} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        throughput(result),
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn(),
    );
}

fn throughput(result: &BenchResult) -> f64 {
    ROWS as f64 * 1e9 / result.median_ns
}

fn improvement(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn percent_change(old: f64, new: f64) -> f64 {
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
