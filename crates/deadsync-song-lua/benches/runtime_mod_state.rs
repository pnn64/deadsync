use deadsync_song_lua::{
    RuntimeModEaseEntry, SongLuaTimeUnit, runtime_mod_state_updates_for_bench,
    runtime_mod_state_updates_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 50_000;
const ENTRY_COUNT: usize = 96;

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

// SAFETY: all allocation operations are forwarded unchanged to `System`;
// atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let output = unsafe { System.alloc(layout) };
        if !output.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        output
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let output = unsafe { System.realloc(ptr, old, new_size) };
        if !output.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        output
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

type Update = fn(&[RuntimeModEaseEntry]) -> u64;

fn main() {
    let entries = benchmark_entries();
    assert_eq!(
        runtime_mod_state_updates_legacy_for_bench(&entries),
        runtime_mod_state_updates_for_bench(&entries)
    );

    let old = measure(&entries, runtime_mod_state_updates_legacy_for_bench);
    let new = measure(&entries, runtime_mod_state_updates_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("Song Lua modifier state construction ({ENTRY_COUNT} entries x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
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

fn benchmark_entries() -> Vec<RuntimeModEaseEntry> {
    const TARGETS: [&str; 8] = [
        "Zoom",
        "Dark",
        "Reverse",
        "Bumpy",
        "ConfusionOffset",
        "Mini",
        "Drunk",
        "Stealth",
    ];
    (0..ENTRY_COUNT)
        .map(|index| RuntimeModEaseEntry {
            unit: SongLuaTimeUnit::Beat,
            start: index as f32,
            limit: 1.0,
            easing: "linear".to_owned(),
            to: (index % 7) as f32 * 0.125,
            target: TARGETS[index % TARGETS.len()].to_owned(),
            start_val: (index % 23 == 0).then_some(0.25),
            opt1: None,
            opt2: None,
            player: match index % 5 {
                0 => Some(1),
                1 => Some(2),
                _ => None,
            },
            add: index % 3 == 0,
        })
        .collect()
}

fn measure(entries: &[RuntimeModEaseEntry], update: Update) -> BenchResult {
    for _ in 0..500 {
        black_box(update(black_box(entries)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(update(black_box(entries))) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (ENTRY_COUNT * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/entry {:>7.2} cycles/entry {:>7.1} Mentries/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per entry, {:.1} bytes/entry",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
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
