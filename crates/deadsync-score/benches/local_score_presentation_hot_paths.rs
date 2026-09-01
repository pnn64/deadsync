use deadsync_core::input::InputSource;
use deadsync_score::{
    Grade, LeaderboardEntry, LocalReplayEdge, LocalScoreEntry, LocalScoreHeader,
    LocalScoreProfileSource, MachineReplayEntry, benchmark_local_score_headers_reference,
    benchmark_local_score_headers_reused, encode_local_score_entry, grade_to_code,
    machine_leaderboard_local_from_profiles, machine_leaderboard_local_from_profiles_reference,
    machine_replays_local_from_profiles, machine_replays_local_from_profiles_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CHART_HASH: &str = "0123456789abcdef0123456789abcdef01234567";
const PROFILES: usize = 3;
const PLAYS_PER_PROFILE: usize = 96;
const FILES: usize = PROFILES * PLAYS_PER_PROFILE;
const REPLAY_EDGES: usize = 192;
const MAX_ENTRIES: usize = 8;
const SAMPLES: usize = 9;

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

// SAFETY: allocation operations delegate unchanged to `System`; the relaxed
// counters only observe successful calls while this single-threaded bench runs.
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

fn measure(
    iterations: usize,
    files_per_op: usize,
    mut operation: impl FnMut() -> u64,
) -> BenchResult {
    for _ in 0..3 {
        black_box(operation());
    }

    let batch = iterations.div_ceil(SAMPLES).max(1);
    let sample_count = iterations.div_ceil(batch);
    let mut sample_ns = Vec::with_capacity(sample_count);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..sample_count {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let allocation_runs = (iterations / 4).max(1);
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..allocation_runs {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_runs = batch * sample_count;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_runs as f64,
        p95_ns: sample_ns[sample_ns.len() * 95 / 100],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_runs as f64),
        files_per_second: measured_runs as f64 * files_per_op as f64 / seconds,
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
         {:>7.3} Mfile/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
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

struct Fixture {
    root: PathBuf,
    profiles: Vec<LocalScoreProfileSource>,
    paths: Vec<PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-local-presentation-bench-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("benchmark directory should be creatable");

        let mut profiles = Vec::with_capacity(PROFILES);
        let mut paths = Vec::with_capacity(FILES);
        for profile in 0..PROFILES {
            let profile_root = root.join(format!("p{profile}"));
            fs::create_dir_all(&profile_root).expect("profile directory should be creatable");
            profiles.push(LocalScoreProfileSource {
                root: profile_root.clone(),
                initials: format!("P{profile:02}"),
                display_name: format!("Profile {profile:02}"),
            });
            for play in 0..PLAYS_PER_PROFILE {
                let played_at_ms = 1_700_000_000_000 + (profile * 10_000 + play) as i64;
                let score_percent = 0.70 + ((play * 37 + profile * 17) % 299) as f64 / 1_000.0;
                let path = profile_root.join(format!("{CHART_HASH}-{played_at_ms}.bin"));
                let bytes = encode_local_score_entry(&score_entry(
                    played_at_ms,
                    score_percent,
                    play.is_multiple_of(89),
                ))
                .expect("benchmark score should encode");
                fs::write(&path, bytes).expect("benchmark score should be writable");
                paths.push(path);
            }
        }
        Self {
            root,
            profiles,
            paths,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn score_entry(played_at_ms: i64, score_percent: f64, failed: bool) -> LocalScoreEntry {
    LocalScoreEntry {
        version: deadsync_score::LOCAL_SCORE_VERSION,
        played_at_ms,
        music_rate: 1.0,
        score_percent,
        grade_code: grade_to_code(if failed { Grade::Failed } else { Grade::Tier03 }),
        lamp_index: Some(2),
        lamp_judge_count: Some(4),
        ex_score_percent: score_percent * 100.0,
        hard_ex_score_percent: score_percent * 100.0,
        judgment_counts: [100, 4, 3, 2, 1, 0],
        holds_held: 5,
        holds_total: 6,
        rolls_held: 7,
        rolls_total: 8,
        mines_avoided: 9,
        mines_total: 10,
        hands_achieved: 11,
        fail_time: failed.then_some(42.0),
        beat0_time_ns: -250_000_000,
        replay: (0..REPLAY_EDGES)
            .map(|edge| {
                LocalReplayEdge::new(
                    1_000_000_000 + edge as i64 * 8_000_000,
                    (edge % 8) as u8,
                    edge.is_multiple_of(2),
                    if edge.is_multiple_of(3) {
                        InputSource::Gamepad
                    } else {
                        InputSource::Keyboard
                    },
                )
            })
            .collect(),
    }
}

fn checksum_headers(headers: &[LocalScoreHeader]) -> u64 {
    headers.iter().fold(0xcbf2_9ce4_8422_2325, |hash, header| {
        hash.rotate_left(7)
            ^ header.score_percent.to_bits()
            ^ (header.played_at_ms as u64).rotate_left(19)
            ^ u64::from(header.grade_code)
    })
}

fn checksum_text(mut hash: u64, value: &str) -> u64 {
    for byte in value.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn checksum_leaderboard(entries: &[LeaderboardEntry]) -> u64 {
    entries.iter().fold(0xcbf2_9ce4_8422_2325, |hash, entry| {
        let hash = checksum_text(hash ^ entry.score.to_bits(), &entry.name);
        checksum_text(hash, entry.machine_tag.as_deref().unwrap_or(""))
    })
}

fn checksum_replays(entries: &[MachineReplayEntry]) -> u64 {
    entries.iter().fold(0xcbf2_9ce4_8422_2325, |hash, entry| {
        let mut hash = checksum_text(hash ^ entry.score.to_bits(), &entry.name);
        for edge in &entry.replay {
            hash = hash.rotate_left(9)
                ^ edge.event_music_time_ns as u64
                ^ u64::from(edge.lane_index).rotate_left(27)
                ^ u64::from(edge.pressed);
        }
        hash
    })
}

fn assert_improved_allocations(title: &str, old: &BenchResult, new: &BenchResult) {
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title} should allocate fewer times"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title} should produce less allocator churn"
    );
}

fn main() {
    let fixture = Fixture::new();

    let old = measure(45, FILES, || {
        checksum_headers(black_box(&benchmark_local_score_headers_reference(
            black_box(&fixture.paths),
        )))
    });
    let new = measure(45, FILES, || {
        checksum_headers(black_box(&benchmark_local_score_headers_reused(black_box(
            &fixture.paths,
        ))))
    });
    print_pair("reuse one header read buffer", &old, &new);
    assert_improved_allocations("header reads", &old, &new);

    let old = measure(36, FILES, || {
        checksum_leaderboard(black_box(
            &machine_leaderboard_local_from_profiles_reference(
                black_box(&fixture.profiles),
                CHART_HASH,
                MAX_ENTRIES,
                true,
            ),
        ))
    });
    let new = measure(36, FILES, || {
        checksum_leaderboard(black_box(&machine_leaderboard_local_from_profiles(
            black_box(&fixture.profiles),
            CHART_HASH,
            MAX_ENTRIES,
            true,
        )))
    });
    print_pair("borrow and bound leaderboard candidates", &old, &new);
    assert_improved_allocations("leaderboard ranking", &old, &new);

    let old = measure(12, FILES, || {
        checksum_replays(black_box(&machine_replays_local_from_profiles_reference(
            black_box(&fixture.profiles),
            CHART_HASH,
            MAX_ENTRIES,
        )))
    });
    let new = measure(12, FILES, || {
        checksum_replays(black_box(&machine_replays_local_from_profiles(
            black_box(&fixture.profiles),
            CHART_HASH,
            MAX_ENTRIES,
        )))
    });
    print_pair("defer full replay decode until after ranking", &old, &new);
    assert_improved_allocations("replay loading", &old, &new);
}
