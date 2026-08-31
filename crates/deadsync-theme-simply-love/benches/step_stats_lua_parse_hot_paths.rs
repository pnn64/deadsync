use deadsync_theme_simply_love::step_stats_gifs::{
    step_stats_actor_command_for_bench, step_stats_actor_command_reference_for_bench,
    step_stats_alignment_name_for_bench, step_stats_alignment_name_reference_for_bench,
    step_stats_numbered_key_for_bench, step_stats_numbered_key_reference_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const REPEATS: usize = 4_096;

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

// SAFETY: all operations delegate unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while its gate is enabled.
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

#[derive(Clone, Copy)]
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

    const fn churn_bytes(self) -> u64 {
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

fn run_timed(op: &mut impl FnMut() -> u64, calls: usize) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(op());
    let elapsed_ns = started.elapsed().as_secs_f64() * 1_000_000_000.0 / calls as f64;
    let elapsed_cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / calls as f64);
    (elapsed_ns, elapsed_cycles, checksum)
}

fn measure_pair(
    calls: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..3 {
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
        let mut record_old = || {
            let (elapsed, cycles, checksum) = run_timed(&mut old_op, calls);
            old_times.push(elapsed);
            if let Some(cycles) = cycles {
                old_cycles.push(cycles);
            }
            old_checksum ^= checksum;
        };
        let mut record_new = || {
            let (elapsed, cycles, checksum) = run_timed(&mut new_op, calls);
            new_times.push(elapsed);
            if let Some(cycles) = cycles {
                new_cycles.push(cycles);
            }
            new_checksum ^= checksum;
        };
        if sample % 2 == 0 {
            record_old();
            record_new();
        } else {
            record_new();
            record_old();
        }
    }
    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);

    let old_allocated = measure_allocations(&mut old_op);
    let new_allocated = measure_allocations(&mut new_op);
    let row = |times: Vec<f64>, cycles: Vec<f64>, allocated, checksum| BenchResult {
        median_ns: percentile(&times, 50),
        p95_ns: percentile(&times, 95),
        median_cycles: (!cycles.is_empty()).then(|| percentile(&cycles, 50)),
        allocated,
        checksum,
    };
    (
        row(old_times, old_cycles, old_allocated, old_checksum),
        row(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn measure_allocations(op: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn run_numbered_keys(fixture: &[(&str, &str)], reference: bool) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..REPEATS {
        for &(key, prefix) in fixture {
            let parsed = if reference {
                step_stats_numbered_key_reference_for_bench(black_box(key), black_box(prefix))
            } else {
                step_stats_numbered_key_for_bench(black_box(key), black_box(prefix))
            };
            checksum = checksum.rotate_left(5) ^ parsed.unwrap_or(usize::MAX) as u64;
        }
    }
    checksum
}

fn run_names(fixture: &[&str], mut parse: impl FnMut(&str) -> u64) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..REPEATS {
        for value in fixture {
            checksum = checksum.rotate_left(5) ^ parse(black_box(value));
        }
    }
    checksum
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>8.2} ns/call  {:>8.2} ns p95  {:>8.2} cycles  \
         {:>8.2} Mops/s  {:>7} alloc  {:>4} realloc  {:>7} free  \
         {:>10} B alloc  {:>10} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        1_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn gate_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title}: behavior diverged");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "gain {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        reduction(old.median_ns, new.median_ns),
        reduction(old.p95_ns, new.p95_ns),
        reduction(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        reduction(old.allocated.allocs as f64, new.allocated.allocs as f64),
        reduction(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        reduction(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    assert!(new.p95_ns < old.p95_ns, "{title}: p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title}: cycles regressed");
    }
    assert!(
        old.allocated.allocs > 0,
        "{title}: reference did not allocate"
    );
    assert_eq!(new.allocated.allocs, 0, "{title}: optimized path allocated");
    assert_eq!(new.allocated.reallocs, 0, "{title}: optimized path grew");
    assert_eq!(new.allocated.frees, 0, "{title}: optimized path freed");
    assert_eq!(
        new.allocated.allocated_bytes, 0,
        "{title}: optimized path allocated bytes"
    );
    assert_eq!(
        new.allocated.churn_bytes(),
        0,
        "{title}: optimized path had allocation churn"
    );
}

fn reduction(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn main() {
    let numbered_keys = [
        ("Frame0000", "frame"),
        ("frame0001", "frame"),
        ("FRAME0015", "frame"),
        ("DeLaY0000", "delay"),
        ("delay0001", "delay"),
        ("DELAY4095", "delay"),
        ("frame", "frame"),
        ("frame-1", "frame"),
        ("frame12tail", "frame"),
        ("Texture", "frame"),
        ("Frames", "frame"),
        ("\u{00e9}frame12", "frame"),
    ];
    let calls = numbered_keys.len() * REPEATS;
    let (old, new) = measure_pair(
        calls,
        || run_numbered_keys(&numbered_keys, true),
        || run_numbered_keys(&numbered_keys, false),
    );
    gate_pair("frame/delay numbered-key parsing", &old, &new);

    let alignments = [
        "left",
        "top",
        "center",
        "middle",
        "right",
        "bottom",
        "LEFT",
        "Center",
        "BoTtOm",
        "baseline",
        " left ",
        "\u{00e9}center",
    ];
    let calls = alignments.len() * REPEATS;
    let (old, new) = measure_pair(
        calls,
        || {
            run_names(&alignments, |value| {
                step_stats_alignment_name_reference_for_bench(value)
                    .map_or(u64::MAX, |value| u64::from(value.to_bits()))
            })
        },
        || {
            run_names(&alignments, |value| {
                step_stats_alignment_name_for_bench(value)
                    .map_or(u64::MAX, |value| u64::from(value.to_bits()))
            })
        },
    );
    gate_pair("Lua alignment-name parsing", &old, &new);

    let actor_commands = [
        "effectclock",
        "x",
        "y",
        "xy",
        "addx",
        "addy",
        "zoom",
        "halign",
        "align",
        "cropleft",
        "cropright",
        "croptop",
        "cropbottom",
        "EffectClock",
        "CropLeft",
        "CROPBOTTOM",
        "rotatez",
    ];
    let calls = actor_commands.len() * REPEATS;
    let (old, new) = measure_pair(
        calls,
        || {
            run_names(&actor_commands, |method| {
                step_stats_actor_command_reference_for_bench(method).map_or(u64::MAX, u64::from)
            })
        },
        || {
            run_names(&actor_commands, |method| {
                step_stats_actor_command_for_bench(method).map_or(u64::MAX, u64::from)
            })
        },
    );
    gate_pair("Lua actor-command dispatch", &old, &new);
}
