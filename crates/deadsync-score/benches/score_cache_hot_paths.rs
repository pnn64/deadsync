use deadsync_score::{
    CachedScore, Grade, ItlFileData, ScoreCacheAccess, ScoreCacheRuntimeKind,
    ScoreCacheRuntimeResult, ScoreProfilePaths, runtime_cached_best_itg_scores,
    runtime_cached_itl_song_folder_unlocked, runtime_cached_itl_song_folders_unlocked,
    runtime_lock_score_caches, runtime_seed_gs_score, runtime_seed_local_itg_score,
    set_itl_score_profile,
};
use smallvec::smallvec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const DIAGNOSTIC_ITERS: usize = 2_000_000;
const BATCH_ITERS: usize = 100_000;
const UNLOCK_ITERS: usize = 100_000;
const QUERY_COUNT: usize = 38;
const PROFILE: &str = "score-cache-hot-path-benchmark";
const UNLOCK_P1: &str = "score-cache-unlock-p1";
const UNLOCK_P2: &str = "score-cache-unlock-p2";

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    dealloc_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            dealloc_bytes: self.dealloc_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocator operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.dealloc_bytes
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
    deallocs: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    dealloc_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            dealloc_bytes: self.dealloc_bytes - before.dealloc_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.dealloc_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    ops_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(1) {
        black_box(op());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(op()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1_000_000_000.0 / iterations as f64,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        ops_per_second: iterations as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.ops_per_second, new.ops_per_second),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let ops = iterations as f64;
    println!(
        "  {label:<3} {:>9.2} ns/op  {:>9.2} cycles/op  {:>8.3} Mop/s  \
         {:>6.2} alloc/op  {:>6.2} realloc/op  {:>6.2} free/op  {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.ops_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.deallocs as f64 / ops,
        result.allocated.churn_bytes() as f64 / ops,
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

fn legacy_diagnostics(value: CachedScore) -> u64 {
    let access = (
        value,
        vec![
            (
                ScoreCacheRuntimeKind::Local,
                ScoreCacheRuntimeResult::default(),
            ),
            (
                ScoreCacheRuntimeKind::GrooveStats,
                ScoreCacheRuntimeResult::default(),
            ),
            (
                ScoreCacheRuntimeKind::ArrowCloud,
                ScoreCacheRuntimeResult::default(),
            ),
        ],
    );
    let checksum = access.1.len() as u64 ^ access.0.score_percent.to_bits();
    drop(access);
    checksum
}

fn inline_diagnostics(value: CachedScore) -> u64 {
    let access = ScoreCacheAccess {
        value,
        results: smallvec![
            (
                ScoreCacheRuntimeKind::Local,
                ScoreCacheRuntimeResult::default(),
            ),
            (
                ScoreCacheRuntimeKind::GrooveStats,
                ScoreCacheRuntimeResult::default(),
            ),
            (
                ScoreCacheRuntimeKind::ArrowCloud,
                ScoreCacheRuntimeResult::default(),
            ),
        ],
    };
    let checksum = access.results.len() as u64 ^ access.value.score_percent.to_bits();
    drop(access);
    checksum
}

fn score_paths(profile_id: &str) -> ScoreProfilePaths {
    ScoreProfilePaths::new(
        std::env::temp_dir()
            .join("deadsync-score-cache-hot-path-empty")
            .join(profile_id),
    )
}

fn legacy_batch<const N: usize>(queries: &[Option<(&str, &str)>; N]) -> [Option<CachedScore>; N] {
    std::array::from_fn(|index| {
        queries[index].and_then(|(profile_id, chart_hash)| {
            runtime_lock_score_caches().merged(profile_id, chart_hash)
        })
    })
}

fn score_checksum(scores: &[Option<CachedScore>]) -> u64 {
    scores
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (index, score)| {
            let bits = score.map_or(0, |score| {
                score.score_percent.to_bits()
                    ^ u64::from(score.grade.to_sprite_state()).rotate_left(17)
            });
            checksum.wrapping_add(bits.rotate_left(index as u32))
        })
}

fn legacy_unlocks<const N: usize>(folders: &[Option<&str>; N]) -> [[bool; 2]; N] {
    std::array::from_fn(|slot| {
        std::array::from_fn(|side| {
            folders[slot].is_some_and(|folder| {
                runtime_cached_itl_song_folder_unlocked(
                    folder,
                    Some([UNLOCK_P1, UNLOCK_P2][side]),
                    |_| ItlFileData::default(),
                )
            })
        })
    })
}

fn unlock_checksum<const N: usize>(membership: &[[bool; 2]; N]) -> u64 {
    membership
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (slot, sides)| {
            checksum.wrapping_add(
                (u64::from(sides[0]) | (u64::from(sides[1]) << 1)).rotate_left(slot as u32),
            )
        })
}

fn main() {
    let score = CachedScore {
        grade: Grade::Tier02,
        score_percent: 0.9876,
        lamp_index: Some(2),
        lamp_judge_count: Some(3),
    };
    let old_diagnostics = measure(DIAGNOSTIC_ITERS, || legacy_diagnostics(score));
    let new_diagnostics = measure(DIAGNOSTIC_ITERS, || inline_diagnostics(score));
    print_pair(
        "inline score-cache diagnostics (three results)",
        DIAGNOSTIC_ITERS,
        &old_diagnostics,
        &new_diagnostics,
    );

    let hashes: [String; QUERY_COUNT] =
        std::array::from_fn(|index| format!("bench-chart-{index:02}"));
    for (index, hash) in hashes.iter().enumerate() {
        let local = CachedScore {
            grade: Grade::Tier04,
            score_percent: 0.90 + index as f64 / 10_000.0,
            lamp_index: Some(4),
            lamp_judge_count: Some(5),
        };
        let gs = CachedScore {
            grade: Grade::Tier02,
            score_percent: 0.97 + index as f64 / 100_000.0,
            lamp_index: Some(2),
            lamp_judge_count: Some(3),
        };
        let _ = runtime_seed_local_itg_score(PROFILE, hash, local, score_paths);
        let _ = runtime_seed_gs_score(PROFILE, hash, gs, score_paths);
    }
    let queries: [Option<(&str, &str)>; QUERY_COUNT] =
        std::array::from_fn(|index| (index % 5 != 4).then_some((PROFILE, hashes[index].as_str())));
    let legacy_expected = legacy_batch(&queries);
    let new_expected = runtime_cached_best_itg_scores(&queries);
    assert_eq!(legacy_expected, new_expected);
    assert_eq!(
        new_expected[0].map(|score| score.grade),
        Some(Grade::Tier02)
    );

    let old_batch = measure(BATCH_ITERS, || score_checksum(&legacy_batch(&queries)));
    let new_batch = measure(BATCH_ITERS, || {
        score_checksum(&runtime_cached_best_itg_scores(&queries))
    });
    print_pair(
        "fixed 38-query score-cache transaction",
        BATCH_ITERS,
        &old_batch,
        &new_batch,
    );

    let unlock_folders: [String; 19] =
        std::array::from_fn(|index| format!("ITL Unlock Song {index:02}"));
    let unlock_queries: [Option<&str>; 19] =
        std::array::from_fn(|index| (index % 5 != 4).then_some(unlock_folders[index].as_str()));
    let mut p1 = ItlFileData::default();
    let mut p2 = ItlFileData::default();
    for (index, folder) in unlock_folders.iter().enumerate() {
        if index % 2 == 0 {
            p1.unlock_folders.insert(folder.clone(), true);
        }
        if index % 3 == 0 {
            p2.unlock_folders.insert(folder.clone(), true);
        }
    }
    set_itl_score_profile(UNLOCK_P1, p1);
    set_itl_score_profile(UNLOCK_P2, p2);
    let old_unlocks = legacy_unlocks(&unlock_queries);
    let new_unlocks = runtime_cached_itl_song_folders_unlocked(
        &unlock_queries,
        [Some(UNLOCK_P1), Some(UNLOCK_P2)],
        |_| ItlFileData::default(),
    );
    assert_eq!(old_unlocks, new_unlocks);

    let old_unlock = measure(UNLOCK_ITERS, || {
        unlock_checksum(&legacy_unlocks(&unlock_queries))
    });
    let new_unlock = measure(UNLOCK_ITERS, || {
        unlock_checksum(&runtime_cached_itl_song_folders_unlocked(
            &unlock_queries,
            [Some(UNLOCK_P1), Some(UNLOCK_P2)],
            |_| ItlFileData::default(),
        ))
    });
    print_pair(
        "fixed 19-slot / two-profile ITL unlock transaction",
        UNLOCK_ITERS,
        &old_unlock,
        &new_unlock,
    );
}
