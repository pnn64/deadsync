use deadsync_noteskin::actor::{
    actor_argument_lookup_for_bench, actor_argument_lookup_legacy_for_bench,
    actor_helper_lookup_for_bench, actor_helper_lookup_legacy_for_bench,
    actor_key_checksum_for_bench, actor_key_checksum_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 250_000;
const HELPER_NAMES: [&str; 8] = [
    "Pulse",
    "DIFFUSE",
    "already_lowercase",
    "MissingHelper",
    "Tiny",
    "ZoomAndFade",
    "dÃ¶wn",
    "A_VERY_LONG_FUNCTION_NAME_THAT_EXCEEDS_THE_STACK_NORMALIZATION_CAPACITY_BECAUSE_ACTOR_HELPERS_CAN_TECHNICALLY_HAVE_UNBOUNDED_IDENTIFIER_LENGTHS",
];
const ARGUMENTS: [&str; 9] = [
    "Tint",
    "\"ACCENT\"",
    "'ShadowColor'",
    "scale",
    "MissingValue",
    "  GLOW  ",
    "dÃ¶wn",
    "AlreadyLowercase",
    "A_VERY_LONG_ARGUMENT_NAME_THAT_EXCEEDS_THE_STACK_NORMALIZATION_CAPACITY_BECAUSE_ACTOR_HELPERS_CAN_TECHNICALLY_HAVE_UNBOUNDED_IDENTIFIER_LENGTHS",
];
const ACTOR_KEYS: [&str; 12] = [
    "Frame0",
    "fRaMe0001",
    "Delay0",
    "dElAy0012",
    "Texture",
    "UnrelatedValue",
    "Frames",
    "Meshes",
    "InitCommand",
    "PRESSCOMMAND",
    "FrameNope",
    "DÃ¶wnCommand",
];

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation operations are forwarded unchanged to `System`; the
// independent atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let functions = helper_map();
    let scope = scope_map();
    let colors = color_map();

    for name in HELPER_NAMES {
        assert_eq!(
            actor_helper_lookup_legacy_for_bench(&functions, name),
            actor_helper_lookup_for_bench(&functions, name)
        );
    }
    for raw in ARGUMENTS {
        assert_eq!(
            actor_argument_lookup_legacy_for_bench(&scope, &colors, raw),
            actor_argument_lookup_for_bench(&scope, &colors, raw)
        );
    }
    for key in ACTOR_KEYS {
        assert_eq!(
            actor_key_checksum_legacy_for_bench(key),
            actor_key_checksum_for_bench(key)
        );
    }

    compare(
        "helper-name lookup",
        HELPER_NAMES.len(),
        || helper_checksum(&functions, actor_helper_lookup_legacy_for_bench),
        || helper_checksum(&functions, actor_helper_lookup_for_bench),
    );
    compare(
        "scoped argument lookup",
        ARGUMENTS.len(),
        || argument_checksum(&scope, &colors, actor_argument_lookup_legacy_for_bench),
        || argument_checksum(&scope, &colors, actor_argument_lookup_for_bench),
    );
    compare(
        "actor block key classification",
        ACTOR_KEYS.len(),
        || key_checksum(actor_key_checksum_legacy_for_bench),
        || key_checksum(actor_key_checksum_for_bench),
    );
}

fn helper_map() -> HashMap<String, u64> {
    [
        "pulse",
        "diffuse",
        "already_lowercase",
        "tiny",
        "zoomandfade",
        "dÃ¶wn",
        "a_very_long_function_name_that_exceeds_the_stack_normalization_capacity_because_actor_helpers_can_technically_have_unbounded_identifier_lengths",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, key)| (key.to_string(), index as u64))
    .collect()
}

fn scope_map() -> HashMap<String, u64> {
    ["tint", "scale", "alreadylowercase"]
        .into_iter()
        .enumerate()
        .map(|(index, key)| (key.to_string(), index as u64))
        .collect()
}

fn color_map() -> HashMap<String, u64> {
    [
        "accent",
        "shadowcolor",
        "glow",
        "dÃ¶wn",
        "a_very_long_argument_name_that_exceeds_the_stack_normalization_capacity_because_actor_helpers_can_technically_have_unbounded_identifier_lengths",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, key)| (key.to_string(), (index + 10) as u64))
    .collect()
}

fn compare(
    label: &str,
    operations_per_run: usize,
    old_batch: impl FnMut() -> u64,
    new_batch: impl FnMut() -> u64,
) {
    let old = measure(old_batch);
    let new = measure(new_batch);
    assert_eq!(old.checksum, new.checksum);

    println!("{label} ({} cases x {RUNS} runs)", operations_per_run);
    print_result("old", &old, operations_per_run);
    print_result("new", &new, operations_per_run);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn measure(mut batch: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2_000 {
        black_box(batch());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(batch()) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn helper_checksum(
    functions: &HashMap<String, u64>,
    lookup: fn(&HashMap<String, u64>, &str) -> Option<u64>,
) -> u64 {
    HELPER_NAMES
        .iter()
        .enumerate()
        .fold(0_u64, |checksum, (index, name)| {
            checksum.rotate_left(5)
                ^ lookup(black_box(functions), black_box(name)).unwrap_or(u64::MAX)
                ^ index as u64
        })
}

fn argument_checksum(
    scope: &HashMap<String, u64>,
    colors: &HashMap<String, u64>,
    lookup: fn(&HashMap<String, u64>, &HashMap<String, u64>, &str) -> Option<u64>,
) -> u64 {
    ARGUMENTS
        .iter()
        .enumerate()
        .fold(0_u64, |checksum, (index, raw)| {
            checksum.rotate_left(5)
                ^ lookup(black_box(scope), black_box(colors), black_box(raw)).unwrap_or(u64::MAX)
                ^ index as u64
        })
}

fn key_checksum(classify: fn(&str) -> u64) -> u64 {
    ACTOR_KEYS
        .iter()
        .enumerate()
        .fold(0_u64, |checksum, (index, key)| {
            checksum.rotate_left(5) ^ classify(black_box(key)) ^ index as u64
        })
}

fn print_result(label: &str, result: &BenchResult, operations_per_run: usize) {
    let operations = (operations_per_run * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/op {:>7.2} cycles/op {:>7.1} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per op, {:.1} bytes/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
