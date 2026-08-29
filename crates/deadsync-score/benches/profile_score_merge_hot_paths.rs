use deadsync_score::{
    ArrowCloudScore, ArrowCloudScores, CachedScore, Grade, benchmark_merged_profile_scores,
    best_cached_itg_score,
};
use rustc_hash::FxBuildHasher;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::BuildHasher;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CHARTS: usize = 2_048;
const ITERATIONS: usize = 120;
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
    items_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(items_per_op: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..6 {
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

    let allocation_runs = ITERATIONS / 6;
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
        items_per_second: measured_runs as f64 * items_per_op as f64 / seconds,
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
        percent_change(old.items_per_second, new.items_per_second),
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

struct Fixtures {
    local: HashMap<String, CachedScore>,
    gs: HashMap<String, CachedScore>,
    ac: HashMap<String, ArrowCloudScores>,
}

fn fixtures() -> Fixtures {
    let mut local = HashMap::with_capacity(CHARTS);
    let mut gs = HashMap::with_capacity(CHARTS);
    let mut ac = HashMap::with_capacity(CHARTS);
    for chart in 0..CHARTS {
        let hash = format!("{chart:064x}");
        local.insert(
            hash.clone(),
            CachedScore {
                grade: Grade::Tier08,
                score_percent: 0.90 + chart as f64 / 1_000_000.0,
                lamp_index: Some(8),
                lamp_judge_count: Some(9),
            },
        );
        gs.insert(
            hash.clone(),
            CachedScore {
                grade: Grade::Tier03,
                score_percent: 0.97 + chart as f64 / 1_000_000.0,
                lamp_index: Some(3),
                lamp_judge_count: Some(4),
            },
        );
        ac.insert(
            hash,
            ArrowCloudScores {
                itg: Some(ArrowCloudScore {
                    score_percent: 0.99 + chart as f64 / 10_000_000.0,
                    server_grade: None,
                    played_at: None,
                    play_id: Some(chart as i64),
                    is_fail: false,
                }),
                ..ArrowCloudScores::default()
            },
        );
    }
    Fixtures { local, gs, ac }
}

fn collect_sorted<S: BuildHasher>(
    merged: HashMap<String, CachedScore, S>,
) -> Vec<(String, CachedScore)> {
    let mut scores: Vec<_> = merged.into_iter().collect();
    scores.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    scores
}

fn merge_get_mut<S: BuildHasher>(
    local: &HashMap<String, CachedScore>,
    gs: Option<&HashMap<String, CachedScore>>,
    ac: Option<&HashMap<String, ArrowCloudScores>>,
    hasher: S,
) -> Vec<(String, CachedScore)> {
    let capacity = local.len() + gs.map_or(0, HashMap::len) + ac.map_or(0, HashMap::len);
    let mut merged = HashMap::with_capacity_and_hasher(capacity, hasher);
    let mut insert = |chart_hash: &str, score: CachedScore| match merged.get_mut(chart_hash) {
        Some(best) => {
            *best = best_cached_itg_score([Some(*best), Some(score)])
                .expect("two fixture scores always produce a best score");
        }
        None => {
            merged.insert(chart_hash.to_owned(), score);
        }
    };
    for (chart_hash, score) in local {
        insert(chart_hash, *score);
    }
    if let Some(scores) = gs {
        for (chart_hash, score) in scores {
            insert(chart_hash, *score);
        }
    }
    if let Some(scores) = ac {
        for (chart_hash, scores) in scores {
            if let Some(score) = scores.itg {
                insert(chart_hash, score.to_cached_score());
            }
        }
    }
    collect_sorted(merged)
}

fn merge_entry_fx(
    local: &HashMap<String, CachedScore>,
    gs: &HashMap<String, CachedScore>,
    ac: &HashMap<String, ArrowCloudScores>,
) -> Vec<(String, CachedScore)> {
    let mut merged =
        FxMap::with_capacity_and_hasher(local.len() + gs.len() + ac.len(), FxBuildHasher);
    let mut insert = |chart_hash: &str, score: CachedScore| {
        merged
            .entry(chart_hash.to_owned())
            .and_modify(|best| {
                *best = best_cached_itg_score([Some(*best), Some(score)])
                    .expect("two fixture scores always produce a best score");
            })
            .or_insert(score);
    };
    for (chart_hash, score) in local {
        insert(chart_hash, *score);
    }
    for (chart_hash, score) in gs {
        insert(chart_hash, *score);
    }
    for (chart_hash, scores) in ac {
        if let Some(score) = scores.itg {
            insert(chart_hash, score.to_cached_score());
        }
    }
    collect_sorted(merged)
}

fn checksum(scores: &[(String, CachedScore)]) -> u64 {
    scores
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, (key, score)| {
            key.bytes().fold(
                hash ^ score.score_percent.to_bits()
                    ^ u64::from(score.grade.to_sprite_state()).rotate_left(13),
                |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3),
            )
        })
}

fn main() {
    let fixtures = fixtures();

    let old = measure(CHARTS, || {
        checksum(black_box(&merge_get_mut(
            black_box(&fixtures.local),
            None,
            None,
            FxBuildHasher,
        )))
    });
    let new = measure(CHARTS, || {
        checksum(black_box(&benchmark_merged_profile_scores(
            Some(black_box(&fixtures.local)),
            None,
            None,
        )))
    });
    print_pair("single-source direct collection", &old, &new);
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.churn_bytes() < old.allocated.churn_bytes());

    let old = measure(CHARTS * 3, || {
        checksum(black_box(&merge_entry_fx(
            black_box(&fixtures.local),
            black_box(&fixtures.gs),
            black_box(&fixtures.ac),
        )))
    });
    let new = measure(CHARTS * 3, || {
        checksum(black_box(&benchmark_merged_profile_scores(
            Some(black_box(&fixtures.local)),
            Some(black_box(&fixtures.gs)),
            Some(black_box(&fixtures.ac)),
        )))
    });
    print_pair("allocate merged key on miss only", &old, &new);
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.churn_bytes() < old.allocated.churn_bytes());

    let old = measure(CHARTS * 3, || {
        checksum(black_box(&merge_get_mut(
            black_box(&fixtures.local),
            Some(black_box(&fixtures.gs)),
            Some(black_box(&fixtures.ac)),
            RandomState::new(),
        )))
    });
    let new = measure(CHARTS * 3, || {
        checksum(black_box(&benchmark_merged_profile_scores(
            Some(black_box(&fixtures.local)),
            Some(black_box(&fixtures.gs)),
            Some(black_box(&fixtures.ac)),
        )))
    });
    print_pair("FxHash for merged chart scores", &old, &new);
}
