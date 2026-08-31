use deadsync_online::lobbies::{
    DefaultMachineStateCacheForBench, LocalLobbyPlayer, MachinePlayerStats,
    MachineStateSignatureCacheForBench, Snapshot, can_update_machine_state,
    local_lobby_machine_state_value, machine_state_signature, machine_state_update_command,
};
use deadsync_profile::PlayerSide;
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const WARMUPS: usize = 5;
const OPS_PER_SAMPLE: usize = 4;
const BATCH: usize = 128;

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

// SAFETY: all operations delegate unchanged to `System`; counters are enabled
// only around this benchmark's single-threaded measured batches.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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
    let mut old_checksum = 0;
    let mut new_checksum = 0;
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample.is_multiple_of(2) {
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
    for _ in 0..OPS_PER_SAMPLE {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9 / OPS_PER_SAMPLE as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS_PER_SAMPLE as f64);
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

fn disconnected_batch(snapshot: &Snapshot, old: bool) -> u64 {
    let stats = MachinePlayerStats {
        score: Some(98.75),
        ex_score: Some(1943.0),
        ..MachinePlayerStats::default()
    };
    let mut sends = 0u64;
    for _ in 0..BATCH {
        if old {
            let state = local_lobby_machine_state_value(
                LocalLobbyPlayer {
                    side: PlayerSide::P1,
                    display_name: "Alice",
                    joined: true,
                    screen_name: "ScreenGameplay",
                    ready: true,
                    stats: Some(&stats),
                },
                LocalLobbyPlayer {
                    side: PlayerSide::P2,
                    display_name: "Bob",
                    joined: false,
                    screen_name: "ScreenGameplay",
                    ready: false,
                    stats: None,
                },
                PlayerSide::P1,
            );
            sends += u64::from(
                machine_state_update_command(
                    snapshot,
                    None,
                    &state,
                    "ScreenGameplay",
                    true,
                    false,
                    Some(stats.clone()),
                    None,
                )
                .is_some(),
            );
        } else {
            sends += u64::from(black_box(can_update_machine_state(black_box(snapshot))));
        }
    }
    sends
}

fn old_signature_batch(state: &Value, last: &mut Option<String>) -> u64 {
    let mut changed = 0u64;
    for _ in 0..BATCH {
        let signature = machine_state_signature("ROOM", state);
        if last.as_deref() != Some(signature.as_str()) {
            *last = Some(signature);
            changed += 1;
        }
    }
    changed
}

fn new_signature_batch(state: &Value, cache: &mut MachineStateSignatureCacheForBench) -> u64 {
    let mut changed = 0u64;
    for _ in 0..BATCH {
        changed += u64::from(cache.update("ROOM", state));
    }
    changed
}

struct OldDefaultInput {
    joined_code: String,
    screen_name: String,
    p1_stats: MachinePlayerStats,
    display_names: [String; 2],
}

fn old_default_batch(
    last_signature: &mut Option<String>,
    last_input: &mut Option<OldDefaultInput>,
) -> u64 {
    let mut changed = 0u64;
    for index in 0..BATCH {
        let stats = MachinePlayerStats {
            score: Some(if index.is_multiple_of(2) { 98.5 } else { 98.6 }),
            ex_score: Some(1943.0 + index as f32),
            ..MachinePlayerStats::default()
        };
        let input = OldDefaultInput {
            joined_code: "ROOM".to_string(),
            screen_name: "ScreenGameplay".to_string(),
            p1_stats: stats,
            display_names: ["Alice".to_string(), "Bob".to_string()],
        };
        let state = local_lobby_machine_state_value(
            LocalLobbyPlayer {
                side: PlayerSide::P1,
                display_name: input.display_names[0].as_str(),
                joined: true,
                screen_name: input.screen_name.as_str(),
                ready: true,
                stats: Some(&input.p1_stats),
            },
            LocalLobbyPlayer {
                side: PlayerSide::P2,
                display_name: input.display_names[1].as_str(),
                joined: false,
                screen_name: input.screen_name.as_str(),
                ready: false,
                stats: None,
            },
            PlayerSide::P1,
        );
        let signature = machine_state_signature(input.joined_code.as_str(), &state);
        if last_signature.as_deref() != Some(signature.as_str()) {
            let command_screen = input.screen_name.clone();
            changed = changed.wrapping_add(command_screen.len() as u64 + 1);
            *last_signature = Some(signature);
        }
        *last_input = Some(input);
    }
    changed
}

fn new_default_batch(cache: &mut DefaultMachineStateCacheForBench) -> u64 {
    let mut changed = 0u64;
    for index in 0..BATCH {
        let stats = MachinePlayerStats {
            score: Some(if index.is_multiple_of(2) { 98.5 } else { 98.6 }),
            ex_score: Some(1943.0 + index as f32),
            ..MachinePlayerStats::default()
        };
        if cache.update(
            "ROOM",
            "ScreenGameplay",
            true,
            false,
            Some(&stats),
            None,
            PlayerSide::P1,
            [true, false],
            ["Alice", "Bob"],
        ) {
            let command_screen = "ScreenGameplay".to_string();
            changed = changed.wrapping_add(command_screen.len() as u64 + 1);
        }
    }
    changed
}

fn run_workload(name: &str, old_op: impl FnMut() -> u64, new_op: impl FnMut() -> u64) {
    let (old, new) = measure_pair(old_op, new_op);
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");

    println!("{name}");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% p95  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% frees  {:>7.2}% bytes  {:>7.2}% churn",
        improvement(old.median_ns, new.median_ns),
        improvement(old.p95_ns, new.p95_ns),
        improvement(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(throughput(&old), throughput(&new)),
        improvement(old.allocated.allocs as f64, new.allocated.allocs as f64),
        improvement(old.allocated.reallocs as f64, new.allocated.reallocs as f64),
        improvement(old.allocated.frees as f64, new.allocated.frees as f64),
        improvement(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        improvement(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );

    assert!(new.median_ns < old.median_ns, "{name} median regressed");
    assert!(new.p95_ns < old.p95_ns, "{name} p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{name} cycles regressed");
    }
    assert!(new.allocated.allocs < old.allocated.allocs, "{name} allocs");
    assert!(
        new.allocated.reallocs <= old.allocated.reallocs,
        "{name} reallocs"
    );
    assert!(new.allocated.frees < old.allocated.frees, "{name} frees");
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{name} allocated bytes"
    );
    assert!(
        new.allocated.churn() < old.allocated.churn(),
        "{name} allocation churn"
    );
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>12.0} items/s  \
         {:>6} alloc  {:>6} realloc  {:>6} free  {:>10} B alloc  {:>10} B churn",
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
    BATCH as f64 * 1e9 / result.median_ns
}

fn improvement(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

fn main() {
    let disconnected = Snapshot::default();
    run_workload(
        "disconnected preflight gate",
        || disconnected_batch(black_box(&disconnected), true),
        || disconnected_batch(black_box(&disconnected), false),
    );

    let state = local_lobby_machine_state_value(
        LocalLobbyPlayer {
            side: PlayerSide::P1,
            display_name: "Alice",
            joined: true,
            screen_name: "ScreenGameplay",
            ready: true,
            stats: Some(&MachinePlayerStats {
                score: Some(98.75),
                ex_score: Some(1943.0),
                ..MachinePlayerStats::default()
            }),
        },
        LocalLobbyPlayer {
            side: PlayerSide::P2,
            display_name: "Bob",
            joined: false,
            screen_name: "ScreenGameplay",
            ready: false,
            stats: None,
        },
        PlayerSide::P1,
    );
    let mut old_signature = None;
    let mut new_signature = MachineStateSignatureCacheForBench::default();
    run_workload(
        "repeated generic signature",
        || old_signature_batch(black_box(&state), &mut old_signature),
        || new_signature_batch(black_box(&state), &mut new_signature),
    );

    let mut old_default_signature = None;
    let mut old_default_input = None;
    let mut new_default_cache = DefaultMachineStateCacheForBench::default();
    run_workload(
        "changing default machine state",
        || old_default_batch(&mut old_default_signature, &mut old_default_input),
        || new_default_batch(&mut new_default_cache),
    );
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
