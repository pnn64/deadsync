use deadsync_noteskin::runtime::{
    tap_explosion_for_col_for_bench, tap_explosion_for_col_legacy_for_bench,
};
use deadsync_noteskin::{
    ExplosionAnimation, ExplosionState, TapExplosion, TapExplosionLayer, TapExplosionMap,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LOOKUP_RUNS: usize = 500_000;
const BUILD_RUNS: usize = 100_000;
const KEYS: [&str; 14] = [
    "W1",
    "W1Bright",
    "W2",
    "W2Bright",
    "W3",
    "W3Bright",
    "W4",
    "W4Bright",
    "W5",
    "W5Bright",
    "Miss",
    "Held",
    "HeldBright",
    "Custom",
];
const QUERIES: [(usize, &str, bool); 10] = [
    (0, "W1", false),
    (0, "W1", true),
    (1, "W2", true),
    (2, "W4", false),
    (3, "W5", true),
    (4, "Held", true),
    (2, "Miss", true),
    (1, "Custom", false),
    (8, "W3", true),
    (0, "Missing", false),
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

struct LegacyFixture {
    default: HashMap<String, TapExplosion<u32>>,
    by_col: Vec<HashMap<String, TapExplosion<u32>>>,
}

struct CurrentFixture {
    default: TapExplosionMap<u32>,
    by_col: Vec<TapExplosionMap<u32>>,
}

fn main() {
    let templates = templates();
    let legacy = legacy_fixture(&templates);
    let current = current_fixture(&templates);
    assert_eq!(legacy_lookup_batch(&legacy), current_lookup_batch(&current));

    let old_lookup = measure(LOOKUP_RUNS, || legacy_lookup_batch(&legacy));
    let new_lookup = measure(LOOKUP_RUNS, || current_lookup_batch(&current));
    assert_eq!(old_lookup.checksum, new_lookup.checksum);

    println!(
        "tap explosion selection ({} queries x {LOOKUP_RUNS} runs)",
        QUERIES.len()
    );
    print_result(
        "old",
        &old_lookup,
        (QUERIES.len() * LOOKUP_RUNS) as f64,
        "lookup",
    );
    print_result(
        "new",
        &new_lookup,
        (QUERIES.len() * LOOKUP_RUNS) as f64,
        "lookup",
    );
    print_reduction(&old_lookup, &new_lookup);

    let old_build = measure(BUILD_RUNS, || {
        legacy_map(black_box(&templates), black_box(0))
            .values()
            .fold(0_u64, |sum, explosion| sum + u64::from(explosion.slot))
    });
    let new_build = measure(BUILD_RUNS, || {
        current_map(black_box(&templates), black_box(0))
            .values()
            .fold(0_u64, |sum, explosion| sum + u64::from(explosion.slot))
    });
    assert_eq!(old_build.checksum, new_build.checksum);

    println!(
        "tap explosion registry construction ({} entries x {BUILD_RUNS} runs)",
        KEYS.len()
    );
    print_result("old", &old_build, BUILD_RUNS as f64, "registry");
    print_result("new", &new_build, BUILD_RUNS as f64, "registry");
    print_reduction(&old_build, &new_build);
    println!(
        "  registry value size: old {} bytes, new {} bytes",
        size_of::<HashMap<String, TapExplosion<u32>>>(),
        size_of::<TapExplosionMap<u32>>()
    );
}

fn templates() -> Vec<TapExplosion<u32>> {
    (0..KEYS.len() as u32)
        .map(|slot| TapExplosion {
            slot,
            animation: ExplosionAnimation {
                initial: ExplosionState::default(),
                segments: Vec::new(),
                glow: None,
                blend_add: false,
            },
            layers: Arc::from([TapExplosionLayer {
                slot,
                animation: ExplosionAnimation {
                    initial: ExplosionState::default(),
                    segments: Vec::new(),
                    glow: None,
                    blend_add: false,
                },
            }]),
        })
        .collect()
}

fn legacy_map(templates: &[TapExplosion<u32>], seed: u32) -> HashMap<String, TapExplosion<u32>> {
    let mut map = HashMap::new();
    for (index, key) in KEYS.iter().enumerate() {
        let mut explosion = templates[index].clone();
        explosion.slot += seed;
        map.insert((*key).to_owned(), explosion);
    }
    map
}

fn current_map(templates: &[TapExplosion<u32>], seed: u32) -> TapExplosionMap<u32> {
    let mut map = TapExplosionMap::new();
    for (index, key) in KEYS.iter().enumerate() {
        let mut explosion = templates[index].clone();
        explosion.slot += seed;
        map.insert_window(key, explosion);
    }
    map
}

fn legacy_fixture(templates: &[TapExplosion<u32>]) -> LegacyFixture {
    LegacyFixture {
        default: legacy_map(templates, 100),
        by_col: (0..4)
            .map(|col| {
                let mut map = legacy_map(templates, 1_000 + col * 100);
                map.retain(|key, _| {
                    KEYS.iter()
                        .position(|candidate| candidate == key)
                        .is_some_and(|index| !(index + col as usize).is_multiple_of(3))
                });
                map
            })
            .collect(),
    }
}

fn current_fixture(templates: &[TapExplosion<u32>]) -> CurrentFixture {
    CurrentFixture {
        default: current_map(templates, 100),
        by_col: (0..4)
            .map(|col| {
                let mut map = TapExplosionMap::new();
                for (index, key) in KEYS.iter().enumerate() {
                    if (index + col as usize).is_multiple_of(3) {
                        continue;
                    }
                    let mut explosion = templates[index].clone();
                    explosion.slot += 1_000 + col * 100;
                    map.insert_window(key, explosion);
                }
                map
            })
            .collect(),
    }
}

fn legacy_lookup_batch(fixture: &LegacyFixture) -> u64 {
    QUERIES
        .iter()
        .fold(0_u64, |checksum, &(col, window, bright)| {
            let marker = tap_explosion_for_col_legacy_for_bench(
                &fixture.default,
                &fixture.by_col,
                black_box(col),
                black_box(window),
                black_box(bright),
            )
            .map_or(u64::MAX, |explosion| u64::from(explosion.slot));
            checksum.rotate_left(5) ^ marker
        })
}

fn current_lookup_batch(fixture: &CurrentFixture) -> u64 {
    QUERIES
        .iter()
        .fold(0_u64, |checksum, &(col, window, bright)| {
            let marker = tap_explosion_for_col_for_bench(
                &fixture.default,
                &fixture.by_col,
                black_box(col),
                black_box(window),
                black_box(bright),
            )
            .map_or(u64::MAX, |explosion| u64::from(explosion.slot));
            checksum.rotate_left(5) ^ marker
        })
}

fn measure(mut runs: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..1_000 {
        black_box(operation());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    while runs != 0 {
        checksum = checksum.rotate_left(7) ^ black_box(operation()) ^ runs as u64;
        runs -= 1;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult, operations: f64, unit: &str) {
    println!(
        "  {label:<4} {:>8.2} ns/{unit} {:>8.2} cycles/{unit} {:>7.2} M{unit}s/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per {unit}, {:.1} bytes/{unit}",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
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

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
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
